//! `mimz eject std` — write the embedded standard library to a local directory
//! so a project can vendor and customize it (then point `mimz.toml [lib] std`
//! at the directory). See `docs/guide/stdlib/README.md`.

use std::path::Path;
use std::process::ExitCode;

/// Which flavor to write: English canonical or the pure-Tamil twins.
pub(crate) enum EjectFlavor {
    English,
    Tamil,
}

pub(crate) fn eject_std(to: &Path, flavor: EjectFlavor, force: bool) -> ExitCode {
    let tamil = matches!(flavor, EjectFlavor::Tamil);
    match mimz::stdlib::eject_to(to, tamil, force) {
        Ok(written) => {
            for p in &written {
                println!("wrote {}", p.display());
            }
            println!(
                "\nActivate with mimz.toml:\n\n[lib]\nstd = \"{}\"",
                to.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
