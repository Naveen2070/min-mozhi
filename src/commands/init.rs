//! `mimz init <name>` — scaffold a new Min-Mozhi project in `./<name>/`.
//! Creates a documented hello-world source file, a `mimz.toml` manifest, and a
//! `.gitignore`. The module name is derived from the project directory name.

use std::path::PathBuf;
use std::process::ExitCode;

/// Derive a valid module identifier from a project name: PascalCase the
/// alphanumeric runs (`my_counter` → `MyCounter`). Falls back to `Top` if the
/// result wouldn't start with a letter (e.g. a name that is all digits/symbols).
/// Non-ASCII letters are kept as-is — Min-Mozhi identifiers may be Tamil.
fn module_name(project: &str) -> String {
    let mut s = String::new();
    let mut at_boundary = true;
    for ch in project.chars() {
        if ch.is_alphanumeric() {
            if at_boundary {
                s.extend(ch.to_uppercase());
                at_boundary = false;
            } else {
                s.push(ch);
            }
        } else {
            at_boundary = true;
        }
    }
    if s.chars().next().is_some_and(char::is_alphabetic) {
        s
    } else {
        "Top".to_string()
    }
}

/// `mimz init <name>` — scaffold a new project in `./<name>/`: a documented
/// `mimz.toml` and a starter `<name>.mimz` (a free-running counter plus an
/// inline `test` block that passes), so `mimz test` / `mimz compile` work
/// immediately. Refuses to overwrite an existing non-empty directory.
pub(crate) fn init(name: &str, quiet: bool) -> ExitCode {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        eprintln!("error: project name must be a simple name, not a path (got `{name}`)");
        return ExitCode::FAILURE;
    }

    let dir = PathBuf::from(name);
    if dir.exists() {
        let non_empty = std::fs::read_dir(&dir)
            .map(|mut e| e.next().is_some())
            .unwrap_or(true);
        if non_empty {
            eprintln!("error: `{}` already exists and is not empty", dir.display());
            return ExitCode::FAILURE;
        }
    } else if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("error: cannot create `{}`: {e}", dir.display());
        return ExitCode::FAILURE;
    }

    let module = module_name(name);

    let config = "# mimz.toml — per-project defaults for the mimz CLI.\n\
        # Every value here is a default that the matching command-line flag overrides.\n\
        \n\
        # Default diagnostics language: english | tanglish | tamil\n\
        lang = \"english\"\n\
        \n\
        # [compile]\n\
        # emit_testbench = true   # `mimz compile` also writes a _tb.v from test blocks\n\
        \n\
        # [fmt]\n\
        # to = \"english\"          # normalize keyword flavor to this\n\
        # strict = false          # warn when a file mixes keyword flavors\n";

    let design = format!(
        "// {name} — a starter Min-Mozhi design.\n\
        // `mimz test {name}.mimz` runs the test block; `mimz compile {name}.mimz` emits Verilog.\n\
        \n\
        module {module}(WIDTH: int = 4) {{\n\
        \x20 clock clk\n\
        \x20 reset rst\n\
        \n\
        \x20 out count: bits[WIDTH]\n\
        \n\
        \x20 reg value: bits[WIDTH] = 0   // `reg ... = 0` declares the mandatory reset value\n\
        \n\
        \x20 on rise(clk) {{\n\
        \x20   value <- value +% 1        // `+%` is wrapping add (same width, wraps on overflow)\n\
        \x20 }}\n\
        \n\
        \x20 count = value\n\
        }}\n\
        \n\
        test \"counts up\" for {module}(WIDTH: 4) {{\n\
        \x20 rst = 1\n\
        \x20 tick(clk)\n\
        \x20 expect count == 0\n\
        \n\
        \x20 rst = 0\n\
        \x20 tick(clk, 4)\n\
        \x20 expect count == 4\n\
        }}\n"
    );

    let config_path = dir.join("mimz.toml");
    let design_path = dir.join(format!("{name}.mimz"));
    if let Err(e) = std::fs::write(&config_path, config) {
        eprintln!("error: cannot write `{}`: {e}", config_path.display());
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(&design_path, design) {
        eprintln!("error: cannot write `{}`: {e}", design_path.display());
        return ExitCode::FAILURE;
    }

    if !quiet {
        println!("created {name}/");
        println!("  {}", config_path.display());
        println!("  {}", design_path.display());
        println!("\nnext:");
        println!("  cd {name}");
        println!("  mimz test {name}.mimz");
        println!("  mimz compile {name}.mimz");
    }
    ExitCode::SUCCESS
}
