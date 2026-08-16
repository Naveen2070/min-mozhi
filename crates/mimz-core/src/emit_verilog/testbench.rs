//! Emits standard Verilog testbenches from inline `test` blocks.
//!
//! Provides `emit_testbench` which generates a `_tb.v` file containing
//! a Verilog module that instantiates the DUT, drives its inputs,
//! and evaluates `expect` statements using self-checking `$display` messages.

use crate::ast::{Dir, ModuleItem, TestDecl, TestStmt, Type};
use crate::checker::consteval::{self, Env};
use crate::diag::Diag;
use crate::emit_verilog::{Emitter, Project, REPEAT_BUDGET};
use std::collections::HashMap;

/// Sanitizes a string for use as a Verilog identifier.
/// Replaces non-alphanumeric characters with `_` and prefixes with `_` if it starts with a digit.
fn sanitize_verilog_ident(name: &str) -> String {
    if name.is_empty() {
        return String::from("_empty");
    }
    let mut safe = String::with_capacity(name.len() + 1);
    let mut chars = name.chars();
    if let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            safe.push('_');
        }
        if c.is_ascii_alphanumeric() {
            safe.push(c);
        } else {
            safe.push('_');
        }
    }
    for c in chars {
        if c.is_ascii_alphanumeric() {
            safe.push(c);
        } else {
            safe.push('_');
        }
    }
    safe
}

/// Recursively emits test statements.
fn emit_test_stmts(em: &mut Emitter, stmts: &[TestStmt], indent: &str) {
    for stmt in stmts {
        match stmt {
            TestStmt::Drive { name, value } => {
                // Non-blocking, not `=`: found live while verifying Task 8
                // (docs/plan/v0.2-class-closure-round3.local.md) — a
                // blocking drive changing `rst`/an input right after
                // `repeat(N) @(posedge clk)` resumes races the DUT's own
                // `always @(posedge clk)` block reading that SAME signal
                // for the SAME edge (both are active-region processes
                // triggered by the identical event; their relative order
                // is implementation-defined). Confirmed live against real
                // `iverilog`: a `reg` reset asserted for exactly one tick
                // then deasserted with `=` left the register `x` forever
                // — not "eventually correct," genuinely never reset,
                // because the DUT sometimes saw `rst` already deasserted
                // on the very edge that should have caught it high. `<=`
                // defers the actual update to the NBA region, so an
                // active-region reader — the DUT's own synchronous logic
                // — is guaranteed to see the PRE-drive value for the
                // edge that just occurred, the standard, textbook fix for
                // this exact race class.
                let val_str = em.expr(value);
                em.out.push_str(&format!(
                    "{}{} <= {};\n",
                    indent,
                    sanitize_verilog_ident(&name.name),
                    val_str
                ));
                // Task 5 (BUG-64, `docs/plan/v0.2-class-closure-round6.local.md`):
                // a `Drive` not followed by a `Tick` (a comb-only test, or a
                // stimulus change between ticks in a clocked one) leaves
                // nothing to advance simulation time — without a real delay
                // here, the very next statement (another `Drive`, or the
                // `Expect` this stimulus exists to feed) reads the
                // PRE-drive value, since `<=` only schedules the update into
                // the NBA region. Unconditional, not gated on "does this
                // test ever tick a clock": a `Drive` followed by a `Tick`
                // just settles slightly earlier than it otherwise would —
                // harmless, since the next clock edge is always a full
                // period away. Confirmed against real `iverilog`:
                // `shift`/`tested_adder`'s emitted testbenches (no `Tick` at
                // all) reported FAIL on a correct design without this — the
                // "vacuous PASS" variant (an `expect` that never observed
                // the stimulus) is worse and is exactly what this closes,
                // not just the loud FAIL case.
                em.out.push_str(&format!("{indent}#1;\n"));
            }
            TestStmt::Tick { clock, count } => {
                let count_val = count
                    .as_ref()
                    .and_then(|c| match consteval::eval(c, &Env::new()) {
                        Ok(v) => Some(v.to_i128_saturating()),
                        Err(e) => {
                            em.diags.push(e);
                            None
                        }
                    })
                    .unwrap_or(1);
                // Clamp count_val to prevent simulator hangs
                let count_val = count_val.clamp(1, 1_000_000);
                em.out.push_str(&format!(
                    "{}repeat ({}) @(posedge {});\n",
                    indent,
                    count_val,
                    sanitize_verilog_ident(&clock.name)
                ));
                // Task 5 (BUG-64): `repeat (n) @(posedge clk)` RESUMES in
                // the same active region as the DUT's own `always
                // @(posedge clk)` block firing for that identical edge —
                // and an NBA (`<=`, what every `on rise` reg write lowers
                // to) never lands until the active region for this time
                // step is entirely done, regardless of relative ordering
                // between the two. So a statement reading a clocked
                // output/reg IMMEDIATELY after `Tick` resumes always sees
                // the PRE-edge value, deterministically, not a maybe-race —
                // this is the standard "sample at posedge+delta" testbench
                // idiom, not optional. Confirmed against real `iverilog`:
                // `enum_encoding`'s `tick(clk); expect state_bits == …`
                // pairs reported FAIL on a correct design without this.
                em.out.push_str(&format!("{indent}#1;\n"));
            }
            TestStmt::Expect(e) => {
                let cond_str = em.expr(e);
                // BUG-54 (docs/audit/bugs.md): `!(cond)` under plain `==`
                // logical negation is `x` whenever `cond` itself is `x`
                // (any operand unknown), and Verilog's `if` treats `x` as
                // false — so an all-`x` design (a register that never got
                // reset, an output that was never driven) silently prints
                // PASS instead of FAIL. Case-INEQUALITY against `1'b1`
                // (`!==`, 4-state comparison, never itself `x`) fails
                // correctly on `x`/`z` as well as on a plain `0`.
                em.out
                    .push_str(&format!("{}if (({}) !== 1'b1) begin\n", indent, cond_str));
                em.out.push_str(&format!(
                    "{}  $display(\"FAIL: expect %0s failed\", \"{}\");\n",
                    indent,
                    cond_str.replace('\"', "\\\"")
                ));
                em.out.push_str(&format!("{}  $finish;\n", indent));
                em.out.push_str(&format!("{}end\n", indent));
            }
            TestStmt::If { cond, then, els } => {
                let cond_str = em.expr(cond);
                em.out
                    .push_str(&format!("{}if ({}) begin\n", indent, cond_str));
                let next_indent = format!("{}  ", indent);
                emit_test_stmts(em, then, &next_indent);
                if let Some(else_stmts) = els {
                    em.out.push_str(&format!("{}end else begin\n", indent));
                    emit_test_stmts(em, else_stmts, &next_indent);
                }
                em.out.push_str(&format!("{}end\n", indent));
            }
            // `sim` blocks are simulation-only (peripheral emulation) — they
            // have no Verilog testbench equivalent, so real hardware codegen
            // just skips them. Permanent: not a gap to fill later.
            TestStmt::Sim(_) => {}
            // Unreachable on the codegen path: `parse` rejects a tree with any
            // `Error` node, so testbench emission never sees one.
            TestStmt::Error(_) => {}
        }
    }
}

/// Generates a Verilog testbench string for the given inline test declarations.
pub fn emit_testbench(project: &Project, tests: &[&TestDecl]) -> Result<String, Vec<Diag>> {
    let mut em = Emitter {
        project,
        out: String::new(),
        diags: Vec::new(),
        cur_file: 0,
        env: Env::new(),
        module_envs: HashMap::new(),
        repeat_budget: REPEAT_BUDGET,
        clog2_fn_used: false,
        emitting_port: false,
        funcs_used: Vec::new(),
        bundle_sigs: HashMap::new(),
        hoist_counter: 0,
        hoisted_decls: String::new(),
        cur_decls: Default::default(),
        in_fn_body: false,
        fn_hoist_counter: 0,
        fn_hoisted_regs: String::new(),
        fn_hoisted_stmts: Vec::new(),
        cover_ordinals: HashMap::new(),
    };

    em.out.push_str(&format!(
        "// Generated by mimz {} (edition {}) — Min-Mozhi (மின்மொழி) Testbench. Do not edit.\n\n",
        crate::version::COMPILER_VERSION,
        crate::version::current().tag()
    ));

    let mut seen_tb_names: HashMap<String, String> = HashMap::new();

    for test in tests {
        let span = test.span;

        let (dut_file, dut) = match project.resolve_module_with_file(&test.module) {
            Some(v) => v,
            None => {
                em.diags.push(Diag::new(
                    span,
                    format!("module `{}` not found for test", test.module.name.name),
                ));
                continue;
            }
        };

        let safe_name = sanitize_verilog_ident(&test.name);
        let base = if safe_name == "_empty" {
            "test"
        } else {
            &safe_name
        };
        let tb_name = format!("{}_tb", base);

        // Two differently-named tests can sanitize to the same Verilog
        // identifier (e.g. "edge case" and "edge_case" both -> `edge_case_tb`),
        // which would otherwise emit two `module edge_case_tb` blocks into the
        // same file. Catch it as a clear diagnostic instead of broken Verilog.
        if let Some(prev_name) = seen_tb_names.get(&tb_name) {
            em.diags.push(Diag::new(
                span,
                format!(
                    "test \"{}\" and test \"{}\" both sanitize to the Verilog module \
                     name `{tb_name}` — rename one test",
                    prev_name, test.name
                ),
            ));
            continue;
        }
        seen_tb_names.insert(tb_name.clone(), test.name.clone());

        em.out.push_str(&format!("module {};\n", tb_name));

        // Resolve explicit test args first — each may reference an earlier one
        // (e.g. `M(W: 8, DEPTH: W * 2)`), same as `sim::harness::params` — then
        // fall back to the module's own parameter defaults for anything the
        // test didn't override, same order/semantics as
        // `sim::elaborate::elaborate_module`. Without this, a width expression
        // like `bits[W]` fails to resolve whenever a test omits a parameter
        // that has a module-level default.
        let mut test_env = Env::new();
        for a in &test.args {
            match consteval::eval(&a.value, &test_env) {
                Ok(v) => {
                    test_env.insert(a.name.name.clone(), v);
                }
                Err(e) => {
                    em.diags.push(e);
                }
            }
        }
        for p in &dut.params {
            if test_env.contains_key(&p.name.name) {
                continue;
            }
            match &p.default {
                Some(d) => match consteval::eval(d, &test_env) {
                    Ok(v) => {
                        test_env.insert(p.name.name.clone(), v);
                    }
                    Err(e) => em.diags.push(e),
                },
                None => em.diags.push(Diag::new(
                    span,
                    format!(
                        "parameter `{}` has no default — provide a value for it in the test",
                        p.name.name
                    ),
                )),
            }
        }

        // Task 2 (BUG-62(a), GAP-16, docs/plan/v0.2-class-closure-round6.local.md):
        // `cur_decls` used to stay `Default::default()` — always empty —
        // for the whole testbench, so every hoist call site's `infer_kind`
        // saw `None` for a plain DUT signal in a `Drive`/`expect` and
        // silently rendered it unchanged (`expect &extend(y, 8) == 0`
        // rendered `(&(y))`, disagreeing with `mimz test`'s own verdict on
        // the identical design). Installing the DUT's own decls, the same
        // way `module()` installs a module's own, closes that gap; `env`
        // is set to this test's resolved param env first so a parametric
        // port width folds the same way the loop above already resolved it
        // (`test_env`, not the otherwise-always-empty `em.env`).
        em.env = test_env.clone();
        em.cur_decls = std::rc::Rc::new(em.build_decls(&dut.items));

        let mut dut_connections = Vec::new();

        for item in &dut.items {
            match item {
                ModuleItem::Port { dir, name, ty } => {
                    let kind = if *dir == Dir::In { "reg" } else { "wire" };
                    let width_str = match ty {
                        Type::Bit => String::new(),
                        Type::Bits(e) | Type::Signed(e) => {
                            match consteval::eval(e.as_ref(), &test_env) {
                                Ok(v) => {
                                    let v = v.to_i128_saturating();
                                    if v > 1 {
                                        format!("[{}-1:0] ", v)
                                    } else {
                                        String::new()
                                    }
                                }
                                Err(e) => {
                                    em.diags.push(e);
                                    String::new()
                                }
                            }
                        }
                        Type::Named(_) => String::new(),
                        Type::Bundle { .. } => String::new(), // bundle ports are pre-flattened by emit_ports
                        Type::Array { .. } => unreachable!(
                            "array types are rejected by the checker (E0416) for module \
                             ports — a DUT's port list can never legitimately contain one"
                        ),
                    };
                    let signed = if matches!(ty, Type::Signed(_)) {
                        "signed "
                    } else {
                        ""
                    };
                    let safe_port_name = sanitize_verilog_ident(&name.name);
                    em.out.push_str(&format!(
                        "  {} {}{}{};\n",
                        kind, signed, width_str, safe_port_name
                    ));
                    dut_connections.push(format!(".{}({})", safe_port_name, safe_port_name));
                }
                ModuleItem::Clock(c) => {
                    let safe_clock = sanitize_verilog_ident(&c.name);
                    em.out.push_str(&format!("  reg {};\n", safe_clock));
                    dut_connections.push(format!(".{}({})", safe_clock, safe_clock));
                }
                ModuleItem::Reset { name, .. } => {
                    let safe_reset = sanitize_verilog_ident(&name.name);
                    em.out.push_str(&format!("  reg {};\n", safe_reset));
                    dut_connections.push(format!(".{}({})", safe_reset, safe_reset));
                }
                _ => {}
            }
        }

        em.out.push('\n');

        let param_str = if test.args.is_empty() {
            String::new()
        } else {
            let params: Vec<String> = test
                .args
                .iter()
                .map(|a| {
                    let val = em.expr(&a.value);
                    format!(".{}({})", sanitize_verilog_ident(&a.name.name), val)
                })
                .collect();
            format!(" #({})", params.join(", "))
        };

        let space_before_param = if param_str.is_empty() { "" } else { " " };
        // Must agree with the DUT's own declaration header in the
        // companion `.v` file (`module()` in module.rs) — same target
        // module, same emitted identifier, or the testbench would
        // instantiate an undeclared module whenever a collision exists.
        let dut_verilog_name = project.verilog_module_name(dut_file, dut);
        em.out.push_str(&format!(
            "  {}{}{} _dut_inst (\n    {}\n  );\n\n",
            sanitize_verilog_ident(&dut_verilog_name),
            space_before_param,
            param_str,
            dut_connections.join(",\n    ")
        ));

        for item in &dut.items {
            if let ModuleItem::Clock(c) = item {
                let safe_clock = sanitize_verilog_ident(&c.name);
                em.out.push_str(&format!("  initial {} = 0;\n", safe_clock));
                em.out
                    .push_str(&format!("  always #5 {} = ~{};\n", safe_clock, safe_clock));
            }
        }
        em.out.push('\n');

        // Found verifying Task 2/3 (docs/plan/v0.2-class-closure-round6.local.md):
        // installing a real `cur_decls` above means a `Drive`/`expect`
        // expression can genuinely hoist now (a reduction/concat/bit-
        // select operand needing a named wire), pushed into
        // `self.hoisted_decls` exactly like the module emitter does — but
        // nothing here ever flushed that buffer, so every hoisted wire was
        // silently DROPPED, leaving `__mimz_sub_N` referenced in the
        // `expect` but never declared. Confirmed against real `iverilog`
        // that this must be textual-order sensitive even at plain module
        // scope (not just inside a `function`, `module()`'s own reason for
        // its `insert_str` — a wire referenced from an `initial` block
        // before its own declaration is ALSO "declaration after use" in
        // practice): declared here, right before the `initial` block that
        // is this hoist's only possible reader, via the same saved-
        // position `insert_str` `module()` uses. Reset per test (not per
        // whole `emit_testbench` call) so numbering restarts at
        // `__mimz_sub_1` per test module, the same per-scope convention
        // `module()` uses.
        let hoist_pos = em.out.len();

        em.out.push_str("  initial begin\n");
        em.out
            .push_str(&format!("    $dumpfile(\"{}.vcd\");\n", tb_name));
        em.out
            .push_str(&format!("    $dumpvars(0, {});\n", tb_name));

        for item in &dut.items {
            if let ModuleItem::Port { dir, name, .. } = item
                && *dir == Dir::In
            {
                em.out.push_str(&format!(
                    "    {} = 0;\n",
                    sanitize_verilog_ident(&name.name)
                ));
            }
            if let ModuleItem::Reset { name, .. } = item {
                em.out.push_str(&format!(
                    "    {} = 0;\n",
                    sanitize_verilog_ident(&name.name)
                ));
            }
        }

        // BUG-65 (docs/audit/bugs.md): the DUT's own reg/mem power-on
        // `initial` statements (`module/mod.rs`) are SEPARATE `initial`
        // constructs from this testbench's own — Verilog gives no ordering
        // guarantee between different `initial` blocks that all start at
        // time 0, only that each runs to its own first blocking delay
        // before yielding. A test with no `Drive`/`Tick` before its first
        // `Expect` (`std/fifo.mimz`'s "starts empty", checking `empty==1`
        // with zero stimulus) could resume and check BEFORE the DUT's own
        // reg-init `initial` had run at all, reading X. `Drive`/`Tick`'s
        // own `#1` (`emit_test_stmts`) already covers every OTHER case;
        // this one settling delay up front, before the first user
        // statement, is what closes the one they can't reach — a test with
        // neither. Confirmed against real `iverilog`: `fifo`'s "starts
        // empty" and `uart_tx`'s "idles high" tests, both zero-stimulus,
        // reported FAIL at time 0 without this.
        em.out.push_str("    #1;\n");

        emit_test_stmts(&mut em, &test.body, "    ");

        em.out.push_str("    $display(\"PASS\");\n");
        // Task 8 #2 (docs/plan/v0.2-class-closure-round3.local.md): the
        // DUT's `__cover_N_count` registers were incremented and read by
        // nothing — print each one's final hit count here, the one place
        // that actually knows the simulation is about to end (a DUT
        // module has no Verilog-2005-legal hook for that on its own; see
        // `module/mod.rs`'s own note on why `final` isn't it). Read via a
        // hierarchical reference through `_dut_inst`, same convention
        // `tests/icarus.rs`'s own cover tests already use to verify these
        // registers from outside the DUT.
        let mut all_covers: Vec<&crate::ast::CoverStmt> = dut
            .items
            .iter()
            .filter_map(|i| match i {
                ModuleItem::Cover(c) => Some(c),
                _ => None,
            })
            .collect();
        all_covers.extend(crate::emit_verilog::module::collect_on_block_covers(
            &dut.items,
        ));
        if !all_covers.is_empty() {
            let ordinals = crate::emit_verilog::module::build_cover_ordinals(&dut.items);
            em.out.push_str("    `ifndef SYNTHESIS\n");
            // A cover counter's own increment (`always @(cond)` for the
            // comb form, an NBA `<=` inside `on rise` for the clocked
            // form) is not guaranteed visible to a same-time-step read —
            // confirmed live against real `iverilog`: even a read a full
            // clock period after the triggering edge raced and saw 0, not
            // 1, with only `#0` in between (a `reg` written by a
            // DIFFERENT process's NBA is not guaranteed settled just
            // because simulation time has since advanced — a same-delta
            // race, not a same-time one). A real delay, not a zero one,
            // is what settles it.
            em.out.push_str("    #1;\n");
            em.out
                .push_str("    $display(\"---- cover summary ----\");\n");
            for c in &all_covers {
                let ord = ordinals[&c.span.start];
                let label = c
                    .label
                    .clone()
                    .unwrap_or_else(|| format!("cover@{}", c.span.start));
                let label = label.replace('\\', "\\\\").replace('"', "\\\"");
                em.out.push_str(&format!(
                    "    $display(\"  {label}: %0d\", _dut_inst.__cover_{ord}_count);\n"
                ));
            }
            em.out.push_str("    `endif\n");
        }
        em.out.push_str("    $finish;\n");
        em.out.push_str("  end\n");
        if !em.hoisted_decls.is_empty() {
            em.out.insert_str(hoist_pos, &em.hoisted_decls);
            em.hoisted_decls.clear();
        }
        em.hoist_counter = 0;
        em.out.push_str("endmodule\n\n");
    }

    if em.diags.is_empty() {
        Ok(em.out)
    } else {
        Err(em.diags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{checker, lexer, parser};

    #[test]
    fn sanitize_verilog_ident_replaces_invalid_chars() {
        assert_eq!(sanitize_verilog_ident("valid_name"), "valid_name");
        assert_eq!(sanitize_verilog_ident("1invalid"), "_1invalid");
        assert_eq!(sanitize_verilog_ident("has spaces"), "has_spaces");
        assert_eq!(sanitize_verilog_ident("with!sym"), "with_sym");
        assert_eq!(sanitize_verilog_ident(""), "_empty");
    }

    /// The same single-file pipeline `mimz compile --emit-testbench` runs
    /// (`src/commands/compile.rs`): lex, parse, check, transliterate, then emit.
    fn compile_tb(src: &str) -> Result<String, Vec<Diag>> {
        let toks = lexer::lex(src)?;
        let ast = parser::parse(toks)?;
        let mut asts = vec![ast];
        checker::check(&asts)?;
        crate::emit_verilog::transliterate(&mut asts);
        let project = crate::emit_verilog::Project::from_files(&asts)?;
        let tests: Vec<&TestDecl> = asts
            .iter()
            .flat_map(|f| {
                f.items.iter().filter_map(|i| match i {
                    crate::ast::TopItem::Test(t) => Some(t),
                    _ => None,
                })
            })
            .collect();
        emit_testbench(&project, &tests)
    }

    /// BUG-54 (docs/audit/bugs.md): `expect`'s guard used to be
    /// `if (!(cond)) FAIL`. Under Verilog's 4-state logic, `!(x)` is `x`,
    /// and `if` treats `x` as false — so a design that never got reset or
    /// never drove its output silently printed PASS. The fix compares
    /// with case-inequality against `1'b1`, which is never itself `x`, so
    /// it correctly fails on `x`/`z` as well as a plain `0`. Pins the
    /// EMITTED TEXT directly (not a full Icarus round-trip — mimz's own
    /// "every reg/mem has a mandatory init value" design makes a genuine
    /// `x` at an `expect` point unreachable from valid source alone,
    /// reachable in practice only through an emitter-level race like
    /// BUG-51's; `tests/icarus.rs`'s
    /// `emitted_testbench_reset_deassert_does_not_race_the_dut` is the
    /// end-to-end proof that THIS fix makes it actually catch BUG-51's
    /// class again instead of vacuously printing PASS either way).
    #[test]
    fn expect_guard_uses_case_inequality_not_plain_negation() {
        let src = "\
module Fuzz {
  in a: bits[4]
  out y: bits[4]
  y = a
}

test \"expect guard shape\" for Fuzz {
  a = 5
  expect y == 5
}
";
        let tb = compile_tb(src).unwrap_or_else(|d| panic!("expected this to compile: {d:?}"));
        assert!(
            tb.contains("!== 1'b1"),
            "expected the `expect` guard to use case-inequality against \
             1'b1 (fails correctly on x/z), got:\n{tb}"
        );
        assert!(
            !tb.contains("if (!(("),
            "the old plain-negation guard shape (`if (!(cond))`) must not \
             reappear — it silently passes on an x-valued comparison:\n{tb}"
        );
    }

    /// A test that doesn't override a module parameter must still resolve
    /// width expressions using the module's own default — mirrors
    /// `sim::elaborate::elaborate_module`'s override-or-default merge, which
    /// `mimz test`/`mimz sim` already rely on.
    #[test]
    fn test_env_falls_back_to_module_param_defaults() {
        let src = "\
module Adder(WIDTH: int = 8) {
  in a: bits[WIDTH]
  in b: bits[WIDTH]
  out sum: bits[WIDTH + 1]
  sum = a + b
}

test \"adder defaults\" for Adder {
  a = 5
  b = 10
  expect sum == 15
}
";
        let tb = compile_tb(src)
            .unwrap_or_else(|d| panic!("expected the WIDTH default (8) to resolve: {d:?}"));
        assert!(
            tb.contains("[8-1:0]"),
            "expected ports sized by the default WIDTH=8, got:\n{tb}"
        );
    }

    /// A later test argument may reference an earlier one in the same
    /// `for Module(...)` argument list.
    #[test]
    fn test_env_chains_earlier_args() {
        let src = "\
module Adder(WIDTH: int = 8, DOUBLE: int = 1) {
  in a: bits[WIDTH]
  out y: bits[WIDTH]
  y = a
}

test \"chained args\" for Adder(WIDTH: 4, DOUBLE: WIDTH * 2) {
  a = 1
  expect y == 1
}
";
        let tb = compile_tb(src).unwrap_or_else(|d| {
            panic!("expected DOUBLE: WIDTH * 2 to resolve against the already-bound WIDTH: {d:?}")
        });
        assert!(
            tb.contains("[4-1:0]"),
            "expected ports sized by WIDTH=4, got:\n{tb}"
        );
    }

    /// Two tests whose names sanitize to the same Verilog module identifier
    /// must be rejected with a clear diagnostic, not silently emitted as two
    /// `module edge_case_tb` blocks in the same file.
    #[test]
    fn colliding_sanitized_test_names_are_rejected() {
        let src = "\
module Buf {
  in a: bit
  out y: bit
  y = a
}

test \"edge case\" for Buf {
  a = 1
  expect y == 1
}

test \"edge_case\" for Buf {
  a = 0
  expect y == 0
}
";
        let diags = compile_tb(src).expect_err("colliding sanitized names must error");
        assert!(
            diags.iter().any(|d| d.msg.contains("edge_case_tb")),
            "expected a collision diagnostic naming `edge_case_tb`, got: {diags:?}"
        );
    }
}
