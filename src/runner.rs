//! In-memory command runner: run a `mimz` subcommand against a source STRING and
//! get back the text a user would see — no filesystem, no process exit, no
//! stdout. This is the engine behind the browser playground console
//! (`crates/mimz-wasm`) and any embedder.
//!
//! It is single-file (no `import` resolution — there is no filesystem to resolve
//! against) and renders diagnostics in English. The CLI keeps its own thin
//! handlers in `src/commands/`, but borrows the shared argument parsers below so
//! there is one source for `--in` / `--param` / `--sweep` / `--trace` parsing.

use std::collections::BTreeMap;

use unicode_normalization::UnicodeNormalization;

use crate::sim::run::{MAX_SIM_CYCLES, MAX_SWEEP_VECTORS, SimOpts, comb_run, run};
use crate::sim::{comb, elaborate, trace, vcd};
use crate::{ast, checker, diag, emit_verilog, lexer, parser};

/// The cosmetic file name shown in caret headers for in-memory sources.
const NAME: &str = "input.mimz";

// ----------------------------------------------------------- argument parsers
//
// Pure, filesystem-free helpers shared by this runner AND the CLI command
// handlers (which re-export them via `mimz::…`), so there is exactly one parser
// for each flag format.

/// Parse a `u128` literal in decimal, `0x` hex, or `0b` binary.
pub fn parse_u128(s: &str) -> Result<u128, String> {
    let parsed = if let Some(hex) = s.strip_prefix("0x") {
        u128::from_str_radix(hex, 16)
    } else if let Some(bin) = s.strip_prefix("0b") {
        u128::from_str_radix(bin, 2)
    } else {
        s.parse::<u128>()
    };
    parsed.map_err(|_| format!("`{s}` is not a number (use decimal, 0x.., or 0b..)"))
}

/// Parse `name=val,name=val` into a map, applying `val_parser` to each value.
/// An empty string is an empty map.
pub fn parse_bindings<T>(
    s: &str,
    val_parser: impl Fn(&str) -> Result<T, String>,
) -> Result<BTreeMap<String, T>, String> {
    let mut map = BTreeMap::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (name, val) = part
            .split_once('=')
            .ok_or_else(|| format!("expected `name=value`, got `{part}`"))?;
        map.insert(name.trim().to_string(), val_parser(val.trim())?);
    }
    Ok(map)
}

/// Parse a `--sweep name=v1|v2|v3,other=w1|w2` spec into ordered
/// `(name, [values])` pairs: entries split on `,`, the value list on `|`.
pub fn parse_sweep(s: &str) -> Result<Vec<(String, Vec<u128>)>, String> {
    let mut out = Vec::new();
    for entry in s.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (name, vals) = entry
            .split_once('=')
            .ok_or_else(|| format!("expected `name=v1|v2`, got `{entry}`"))?;
        let values = vals
            .split('|')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(parse_u128)
            .collect::<Result<Vec<u128>, String>>()?;
        if values.is_empty() {
            return Err(format!("sweep input `{}` lists no values", name.trim()));
        }
        out.push((name.trim().to_string(), values));
    }
    Ok(out)
}

/// Parse `--steps "a=3,b=5;a=7,b=1"` into explicit per-step input vectors:
/// groups split on `;`, each parsed as `name=val,…` bindings (an empty group is
/// dropped). Bounded by [`MAX_SWEEP_VECTORS`] so a pasted giant table can't hang
/// the tool. Unlike [`parse_sweep`]/[`sweep_vectors`] (a cartesian product), each
/// group is one literal vector — what the playground step table produces.
pub fn parse_steps(s: &str) -> Result<Vec<BTreeMap<String, u128>>, String> {
    let groups: Vec<&str> = s
        .split(';')
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .collect();
    if groups.len() > MAX_SWEEP_VECTORS {
        return Err(format!("--steps lists over {MAX_SWEEP_VECTORS} steps"));
    }
    groups
        .iter()
        .map(|g| parse_bindings(g, parse_u128))
        .collect()
}

/// The input vectors to drive a combinational run: the cartesian product of the
/// `sweep` dimensions, each combination overlaid on the held `base` inputs. No
/// sweep yields a single vector equal to `base`. Errors if the product would
/// exceed [`MAX_SWEEP_VECTORS`] (a large sweep must not OOM/hang the tool).
pub fn sweep_vectors(
    base: &BTreeMap<String, u128>,
    sweep: &[(String, Vec<u128>)],
) -> Result<Vec<BTreeMap<String, u128>>, String> {
    let mut product: usize = 1;
    for (_, values) in sweep {
        product = product
            .checked_mul(values.len())
            .filter(|p| *p <= MAX_SWEEP_VECTORS)
            .ok_or_else(|| format!("--sweep expands to over {MAX_SWEEP_VECTORS} input vectors"))?;
    }

    let mut vectors = vec![base.clone()];
    for (name, values) in sweep {
        let mut next = Vec::with_capacity(vectors.len() * values.len());
        for v in &vectors {
            for val in values {
                let mut m = v.clone();
                m.insert(name.clone(), *val);
                next.push(m);
            }
        }
        vectors = next;
    }
    Ok(vectors)
}

/// Resolve the console-trace scope from the flags, shared by `sim` and `test`.
/// `--signals` (an explicit, validated subset) overrides `--verbose` (all
/// signals), which overrides the default (interface + state). An unknown
/// `--signals` name is a clean error naming `module`.
pub fn trace_scope(
    all: &[String],
    default: &[String],
    verbose: bool,
    signals: &Option<String>,
    module: &str,
) -> Result<Vec<String>, String> {
    match signals {
        Some(list) => {
            let chosen: Vec<String> = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            for s in &chosen {
                if !all.iter().any(|n| n == s) {
                    return Err(format!(
                        "--signals names `{s}`, which is not a signal of `{module}`"
                    ));
                }
            }
            Ok(chosen)
        }
        None if verbose => Ok(all.to_vec()),
        None => Ok(default.to_vec()),
    }
}

// ------------------------------------------------------------------ the runner

/// Run one `mimz` subcommand against `source`, returning the text the user would
/// see. `argv` is the flag list AFTER the command word (e.g.
/// `["--in", "a=1", "--cycles", "8", "--trace"]`).
///
/// `Ok` carries the command's normal output; `Err` carries an error message or
/// rendered diagnostics — both are display-ready for a console/log.
pub fn run_command(source: &str, command: &str, argv: &[&str]) -> Result<String, String> {
    let src: String = source.nfc().collect();
    match command {
        "check" => check(&src, argv),
        "compile" => compile(&src, argv),
        "eval" => eval(&src, argv),
        "ports" => ports(&src, argv),
        "sim" => sim(&src, argv),
        "test" => test(&src, argv),
        other => Err(format!(
            "unknown command `{other}` — try check, compile, eval, ports, sim, or test"
        )),
    }
}

/// Lex + parse an already-NFC-normalized source into one file, rendering any
/// lexer/parser diagnostics to text on failure.
fn parse_source(src: &str) -> Result<Vec<crate::project::LoadedFile>, String> {
    let render = |d: Vec<diag::Diag>| diag::render(&d, src, NAME);
    let toks = lexer::lex(src).map_err(render)?;
    let root_ast = parser::parse(toks).map_err(render)?;

    let mut files = vec![crate::project::LoadedFile {
        path: std::path::PathBuf::from(NAME),
        src: src.to_string(),
        ast: root_ast,
    }];
    let mut loaded_stems = std::collections::HashMap::new();

    let mut i = 0;
    while i < files.len() {
        let imports = files[i].ast.imports.clone();
        let mut target_indices = Vec::new();

        for imp in &imports {
            let is_std = imp
                .path
                .first()
                .is_some_and(|seg| crate::stdlib::is_std_namespace(&seg.name));
            if is_std && imp.path.len() != 2 {
                return Err(
                    "a standard-library import must be `std.<module>` (exactly two segments)"
                        .to_string(),
                );
            } else if is_std {
                let ns = &imp.path[0].name;
                let mod_name = &imp.path[1].name;
                if let Some((m, v)) = crate::stdlib::resolve(ns, mod_name) {
                    let target_idx = if let Some(&idx) = loaded_stems.get(m.stem) {
                        idx
                    } else {
                        let std_src = m.source(v);
                        let std_toks =
                            lexer::lex(std_src).map_err(|d| diag::render(&d, std_src, mod_name))?;
                        let std_ast = parser::parse(std_toks)
                            .map_err(|d| diag::render(&d, std_src, mod_name))?;
                        let idx = files.len();
                        loaded_stems.insert(m.stem, idx);
                        files.push(crate::project::LoadedFile {
                            path: std::path::PathBuf::from(format!("<std::{mod_name}>")),
                            src: std_src.to_string(),
                            ast: std_ast,
                        });
                        idx
                    };
                    target_indices.push(target_idx);
                } else {
                    return Err(format!("unknown standard library module `{ns}.{mod_name}`"));
                }
            } else {
                return Err(
                    "`import` is not supported when compiling a single in-memory source — \
                     the in-browser compiler resolves no files. Paste the imported modules \
                     into this source (only standard library imports like `std.uart_tx` are supported)."
                        .to_string(),
                );
            }
        }
        for (imp, &idx) in files[i].ast.imports.iter().zip(&target_indices) {
            imp.resolved_file.set(Some(idx));
        }
        i += 1;
    }
    Ok(files)
}

/// Pull the value following a `--flag`, advancing the cursor past both.
fn flag_value<'a>(argv: &'a [&'a str], i: &mut usize, flag: &str) -> Result<&'a str, String> {
    let v = argv
        .get(*i + 1)
        .copied()
        .ok_or_else(|| format!("`{flag}` needs a value"))?;
    *i += 2;
    Ok(v)
}

/// `check` — lex, parse, and run the full safety checker; no output written.
fn check(src: &str, _argv: &[&str]) -> Result<String, String> {
    let files = parse_source(src)?;
    let asts: Vec<_> = files.iter().map(|f| f.ast.clone()).collect();
    if let Err(d) = checker::check(&asts) {
        return Err(crate::project::render_diags(&d, &files));
    }
    Ok("OK — no errors.".to_string())
}

/// `compile` — the full pipeline to Verilog. `import` is rejected (single-file,
/// in-memory). This is the one compile implementation; [`crate::compile_string`]
/// is a thin wrapper over it.
fn compile(src: &str, _argv: &[&str]) -> Result<String, String> {
    let files = parse_source(src)?;
    let render = |d: Vec<diag::Diag>| crate::project::render_diags(&d, &files);
    let mut asts: Vec<_> = files.iter().map(|f| f.ast.clone()).collect();
    checker::check(&asts).map_err(render)?;
    emit_verilog::transliterate(&mut asts);
    let project = emit_verilog::Project::from_files(&asts).map_err(render)?;
    emit_verilog::emit(&project, &asts).map_err(render)
}

/// `eval --in a=3,b=5 [--param W=8] [--module M]` — interpret a combinational
/// module and print each output.
fn eval(src: &str, argv: &[&str]) -> Result<String, String> {
    let mut inputs_s = "";
    let mut param_s = "";
    let mut module: Option<String> = None;
    let mut i = 0;
    while i < argv.len() {
        match argv[i] {
            "--in" => inputs_s = flag_value(argv, &mut i, "--in")?,
            "--param" => param_s = flag_value(argv, &mut i, "--param")?,
            "--module" => module = Some(flag_value(argv, &mut i, "--module")?.to_string()),
            other => return Err(format!("unknown eval flag `{other}`")),
        }
    }

    let files = parse_source(src)?;
    let inputs = parse_bindings(inputs_s, parse_u128)?;
    let params = parse_bindings(param_s, |s| parse_u128(s).map(|v| v as i128))?;

    let asts: Vec<_> = files.iter().map(|f| f.ast.clone()).collect();
    let outputs = comb::eval_outputs(&asts, module.as_deref(), &inputs, &params)?;
    let mut out = String::new();
    for o in outputs {
        let kind = if o.signed { "signed" } else { "bits" };
        out.push_str(&format!(
            "{} = {}  ({kind}[{}])\n",
            o.name, o.value, o.width
        ));
    }
    Ok(out)
}

/// Render one elaborated signal as a JSON object: `{"name","width","signed"}`.
/// Signal names are identifiers, so they need no JSON string escaping.
fn signal_json(s: &elaborate::Signal) -> String {
    format!(
        "{{\"name\":\"{}\",\"width\":{},\"signed\":{}}}",
        s.name, s.width.bits, s.width.signed
    )
}

/// `ports [--param W=8] [--module M]` — describe a module's interface as JSON so
/// an embedder (the playground stimulus panel) can build input controls without
/// re-parsing the source: `{"module","clocked","inputs":[…],"outputs":[…]}`.
/// `clocked` distinguishes a design driven over cycles (`--in`/`--cycles`) from a
/// combinational one driven by explicit input vectors (`--steps`).
fn ports(src: &str, argv: &[&str]) -> Result<String, String> {
    let mut param_s = "";
    let mut module: Option<String> = None;
    let mut i = 0;
    while i < argv.len() {
        match argv[i] {
            "--param" => param_s = flag_value(argv, &mut i, "--param")?,
            "--module" => module = Some(flag_value(argv, &mut i, "--module")?.to_string()),
            other => return Err(format!("unknown ports flag `{other}`")),
        }
    }

    let files = parse_source(src)?;
    let params = parse_bindings(param_s, |s| parse_u128(s).map(|v| v as i128))?;
    let asts: Vec<_> = files.iter().map(|f| f.ast.clone()).collect();
    let design = elaborate::elaborate_project(&asts, module.as_deref(), &params)?;

    let join =
        |sigs: &[elaborate::Signal]| sigs.iter().map(signal_json).collect::<Vec<_>>().join(",");
    Ok(format!(
        "{{\"module\":\"{}\",\"clocked\":{},\"inputs\":[{}],\"outputs\":[{}]}}",
        design.module,
        !design.clocks.is_empty(),
        join(&design.inputs),
        join(&design.outputs),
    ))
}

/// `sim [--in …] [--param …] [--sweep …] [--steps …] [--cycles N] [--clock c] [--module M]
/// [--trace[=changes]] [--verbose] [--signals a,b]` — simulate a module. A
/// clocked design runs the default stimulus over `cycles`; a combinational one
/// settles once per input vector — either explicit (`--steps`) or the held
/// `--in` fanned out by `--sweep`.
fn sim(src: &str, argv: &[&str]) -> Result<String, String> {
    let mut inputs_s = "";
    let mut param_s = "";
    let mut sweep_s = "";
    let mut steps_s = "";
    let mut cycles: u64 = 16;
    let mut clock: Option<String> = None;
    let mut module: Option<String> = None;
    let mut trace_style: Option<String> = None;
    let mut verbose = false;
    let mut signals: Option<String> = None;
    let mut want_vcd = false;

    let mut i = 0;
    while i < argv.len() {
        let a = argv[i];
        // `--trace` or `--trace=changes` (the value, if any, rides the same token).
        if a == "--trace" || a.starts_with("--trace=") {
            trace_style = Some(match a.split_once('=') {
                Some((_, style)) => style.to_string(),
                None => "table".to_string(),
            });
            i += 1;
            continue;
        }
        match a {
            "--in" => inputs_s = flag_value(argv, &mut i, "--in")?,
            "--param" => param_s = flag_value(argv, &mut i, "--param")?,
            "--sweep" => sweep_s = flag_value(argv, &mut i, "--sweep")?,
            "--steps" => steps_s = flag_value(argv, &mut i, "--steps")?,
            "--cycles" => {
                let v = flag_value(argv, &mut i, "--cycles")?;
                cycles = v
                    .parse()
                    .map_err(|_| format!("`{v}` is not a valid cycle count"))?;
            }
            "--clock" => clock = Some(flag_value(argv, &mut i, "--clock")?.to_string()),
            "--module" => module = Some(flag_value(argv, &mut i, "--module")?.to_string()),
            "--signals" => signals = Some(flag_value(argv, &mut i, "--signals")?.to_string()),
            "--verbose" => {
                verbose = true;
                i += 1;
            }
            "--vcd" => {
                want_vcd = true;
                i += 1;
            }
            other => return Err(format!("unknown sim flag `{other}`")),
        }
    }

    if cycles == 0 || cycles > MAX_SIM_CYCLES {
        return Err(format!("--cycles must be between 1 and {MAX_SIM_CYCLES}"));
    }

    let files = parse_source(src)?;
    let inputs = parse_bindings(inputs_s, parse_u128)?;
    let params = parse_bindings(param_s, |s| parse_u128(s).map(|v| v as i128))?;
    let sweep = parse_sweep(sweep_s)?;
    // `--steps "a=3,b=5;a=7,b=1"` — explicit per-step input vectors (one `;`-group
    // each), for the playground's combinational step table. Distinct from
    // `--sweep`'s cartesian product, so the two cannot be combined.
    let steps = parse_steps(steps_s)?;
    if !steps.is_empty() && !sweep.is_empty() {
        return Err("--steps and --sweep cannot be combined".to_string());
    }

    let asts: Vec<_> = files.iter().map(|f| f.ast.clone()).collect();
    let design = elaborate::elaborate_project(&asts, module.as_deref(), &params)?;
    // Capture the scope groups + clocked-ness before the run consumes the design.
    let in_names: Vec<String> = design.inputs.iter().map(|s| s.name.clone()).collect();
    let out_names: Vec<String> = design.outputs.iter().map(|s| s.name.clone()).collect();
    let reg_names: Vec<String> = design.regs.iter().map(|r| r.name.clone()).collect();
    let clocked = !design.clocks.is_empty();

    if clocked && !steps.is_empty() {
        return Err(
            "--steps drives a combinational design's input vectors; a clocked design \
             advances over --cycles with held --in values"
                .to_string(),
        );
    }

    let timeline = if clocked {
        let opts = SimOpts {
            clock,
            inputs,
            cycles,
            reset_cycles: 1,
        };
        run(design, &opts)?
    } else if !steps.is_empty() {
        comb_run(design, &steps)?
    } else {
        let vectors = sweep_vectors(&inputs, &sweep)?;
        comb_run(design, &vectors)?
    };
    let steps = timeline.frames.iter().filter(|f| f.cycle.is_some()).count();

    // `--vcd` returns the 2-state VCD document (what the playground waveform
    // viewer parses) instead of a console trace.
    if want_vcd {
        return Ok(vcd::to_vcd(&timeline));
    }

    if let Some(style) = &trace_style {
        let all_names: Vec<String> = timeline.signals.iter().map(|s| s.name.clone()).collect();
        let default: Vec<String> = in_names
            .into_iter()
            .chain(out_names)
            .chain(reg_names)
            .collect();
        let scope = trace_scope(&all_names, &default, verbose, &signals, &timeline.module)?;
        Ok(trace::render(&timeline, style, &scope))
    } else {
        let unit = if clocked {
            "cycle(s)"
        } else {
            "input vector(s)"
        };
        Ok(format!(
            "simulated {steps} {unit} of `{}` — add --trace for a console trace\n",
            timeline.module
        ))
    }
}

/// `test [--filter substr]` — run the source's `test` blocks and report
/// pass/fail. Errs (non-zero, conceptually) if any test fails.
fn test(src: &str, argv: &[&str]) -> Result<String, String> {
    let mut filter: Option<String> = None;
    let mut i = 0;
    while i < argv.len() {
        match argv[i] {
            "--filter" => filter = Some(flag_value(argv, &mut i, "--filter")?.to_string()),
            other => return Err(format!("unknown test flag `{other}`")),
        }
    }

    let files = parse_source(src)?;
    let file = &files[0].ast;
    let decls: Vec<&ast::TestDecl> = file
        .items
        .iter()
        .filter_map(|it| match it {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .filter(|t| filter.as_deref().is_none_or(|f| t.name.contains(f)))
        .collect();

    if decls.is_empty() {
        return Ok("no tests found.\n".to_string());
    }

    let asts: Vec<_> = files.iter().map(|f| f.ast.clone()).collect();
    let mut out = String::new();
    let (mut passed, mut failed) = (0u32, 0u32);
    for decl in decls {
        match crate::sim::harness::run_test(&asts, src, decl) {
            Ok(o) => match o.result {
                crate::sim::harness::TestResult::Pass => {
                    passed += 1;
                    let s = if o.checks == 1 { "check" } else { "checks" };
                    out.push_str(&format!("ok   {} ({} {s})\n", o.name, o.checks));
                }
                crate::sim::harness::TestResult::Fail(msg) => {
                    failed += 1;
                    out.push_str(&format!("FAIL {}\n", o.name));
                    for line in msg.lines() {
                        out.push_str(&format!("       {line}\n"));
                    }
                }
            },
            Err(e) => {
                failed += 1;
                out.push_str(&format!("error in test \"{}\": {e}\n", decl.name));
            }
        }
    }
    out.push_str(&format!("\n{passed} passed, {failed} failed\n"));
    if failed == 0 { Ok(out) } else { Err(out) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_vectors_rejects_an_oversized_product() {
        // SEC: 7 dims x 10 values = 10^7 > MAX_SWEEP_VECTORS — must error before
        // allocating, not OOM/hang.
        let base = BTreeMap::new();
        let dim: Vec<u128> = (0..10).collect();
        let sweep: Vec<(String, Vec<u128>)> =
            (0..7).map(|i| (format!("x{i}"), dim.clone())).collect();
        let err = sweep_vectors(&base, &sweep).unwrap_err();
        assert!(err.contains("input vectors"), "unexpected error: {err}");
    }

    #[test]
    fn sweep_vectors_allows_a_normal_product() {
        let base = BTreeMap::new();
        let sweep = vec![
            ("a".to_string(), vec![0u128, 1]),
            ("b".to_string(), vec![0u128, 1, 2]),
        ];
        let v = sweep_vectors(&base, &sweep).unwrap();
        assert_eq!(v.len(), 6, "2 x 3 cartesian product");
    }

    #[test]
    fn check_reports_ok_and_errors() {
        assert_eq!(
            run_command(
                "module B {\n in a: bit\n out y: bit\n y = a\n}\n",
                "check",
                &[]
            )
            .unwrap(),
            "OK — no errors."
        );
        let err = run_command("module {", "check", &[]).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn eval_runs_a_combinational_module() {
        let src = "module And2 {\n in a: bit\n in b: bit\n out y: bit\n y = a and b\n}\n";
        let out = run_command(src, "eval", &["--in", "a=1,b=1"]).unwrap();
        assert!(out.contains("y = 1"), "got: {out}");
    }

    #[test]
    fn sim_traces_a_clocked_module() {
        let src = "module Counter {\n clock clk\n reset rst\n out count: bits[4]\n \
                   reg value: bits[4] = 0\n on rise(clk) { value <- value +% 1 }\n count = value\n}\n";
        let out = run_command(src, "sim", &["--cycles", "3", "--trace"]).unwrap();
        assert!(
            out.contains("count"),
            "trace should name the output, got: {out}"
        );
    }

    #[test]
    fn ports_describes_a_combinational_interface() {
        let src = "module Add(W: int = 8) {\n in a: bits[W]\n in b: bits[W]\n \
                   out sum: bits[W]\n sum = a +% b\n}\n";
        let json = run_command(src, "ports", &[]).unwrap();
        assert!(json.contains("\"module\":\"Add\""), "got: {json}");
        assert!(json.contains("\"clocked\":false"), "got: {json}");
        assert!(
            json.contains("\"name\":\"a\",\"width\":8,\"signed\":false"),
            "got: {json}"
        );
        assert!(json.contains("\"name\":\"sum\""), "got: {json}");
    }

    #[test]
    fn ports_reports_a_clocked_design() {
        let src = "module Counter {\n clock clk\n reset rst\n out count: bits[4]\n \
                   reg value: bits[4] = 0\n on rise(clk) { value <- value +% 1 }\n count = value\n}\n";
        let json = run_command(src, "ports", &[]).unwrap();
        assert!(json.contains("\"clocked\":true"), "got: {json}");
    }

    #[test]
    fn sim_steps_drives_explicit_vectors() {
        // Three explicit input vectors -> three settled frames in the VCD.
        let src = "module Add {\n in a: bits[8]\n in b: bits[8]\n out sum: bits[8]\n \
                   sum = a +% b\n}\n";
        let vcd =
            run_command(src, "sim", &["--steps", "a=3,b=5;a=7,b=1;a=0,b=2", "--vcd"]).unwrap();
        assert!(vcd.contains("$var"), "expected a VCD, got:\n{vcd}");
        // 8 (3+5) then 8 (7+1) then 2 (0+2) — the last distinct value must appear.
        assert!(
            vcd.contains("b10 "),
            "expected sum=2 (b10) in VCD, got:\n{vcd}"
        );
    }

    #[test]
    fn sim_steps_is_rejected_for_a_clocked_design() {
        let src = "module Counter {\n clock clk\n reset rst\n out count: bits[4]\n \
                   reg value: bits[4] = 0\n on rise(clk) { value <- value +% 1 }\n count = value\n}\n";
        let err = run_command(src, "sim", &["--steps", "x=1", "--vcd"]).unwrap_err();
        assert!(err.contains("clocked"), "got: {err}");
    }

    #[test]
    fn sim_vcd_emits_a_vcd_document() {
        let src = "module Counter {\n clock clk\n reset rst\n out count: bits[4]\n \
                   reg value: bits[4] = 0\n on rise(clk) { value <- value +% 1 }\n count = value\n}\n";
        let vcd = run_command(src, "sim", &["--cycles", "3", "--vcd"]).unwrap();
        assert!(
            vcd.contains("$var") && vcd.contains("$enddefinitions"),
            "expected a VCD document, got:\n{vcd}"
        );
    }
}
