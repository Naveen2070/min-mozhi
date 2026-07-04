use std::path::Path;
use std::process::Command;

fn main() {
    // Only run this in local development environments where git is initialized
    // and we are not running on a CI server.
    if std::env::var("CI").is_err() && Path::new(".git").exists() {
        // Automatically set the hooks path to the tracked `.githooks` directory.
        let status = Command::new("git")
            .args(["config", "core.hooksPath", ".githooks"])
            .status();

        if let Ok(status) = status {
            if status.success() {
                println!("cargo:warning=Git hooks configured successfully to use .githooks/");
            }
        }
    }
}
