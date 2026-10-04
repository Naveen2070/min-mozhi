//! Finds and runs the OSS CAD Suite tools for `mimz build`. With a suite
//! root, a tool runs from `<root>/bin` with `<root>/bin` and `<root>/lib`
//! first on PATH for that child process only (the suite's DLLs and bundled
//! python never reach the global PATH; docs/BUILD.md); otherwise from PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the synthesis tools come from: a suite root, or PATH (`None`).
pub struct Toolchain {
    pub root: Option<PathBuf>,
}

/// Windows Yosys builds crash in `synth_ice40`'s ABC9 step (docs/BUILD.md),
/// so `mimz build` passes `-noabc` there.
pub fn needs_noabc() -> bool {
    cfg!(windows)
}

fn exe(dir: &Path, tool: &str) -> PathBuf {
    dir.join(format!("{tool}{}", std::env::consts::EXE_SUFFIX))
}

impl Toolchain {
    /// `MIMZ_OSS_CAD`, else `[build] toolchain` from `mimz.toml`, else PATH.
    pub fn discover(config_root: Option<&Path>) -> Toolchain {
        let root = std::env::var_os("MIMZ_OSS_CAD")
            .map(PathBuf::from)
            .or_else(|| config_root.map(Path::to_path_buf));
        Toolchain { root }
    }

    /// The executable for `tool`, if it exists.
    pub fn find(&self, tool: &str) -> Option<PathBuf> {
        match &self.root {
            Some(r) => Some(exe(&r.join("bin"), tool)).filter(|p| p.is_file()),
            None => std::env::var_os("PATH")
                .into_iter()
                .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                .map(|d| exe(&d, tool))
                .find(|p| p.is_file()),
        }
    }

    /// A `Command` for `tool`, with the suite's `bin` and `lib` first on the
    /// child's PATH and `YOSYSHQ_ROOT` set when there is a suite root.
    pub fn command(&self, tool: &str) -> Command {
        match &self.root {
            Some(r) => {
                let mut c = Command::new(exe(&r.join("bin"), tool));
                let old = std::env::var_os("PATH").unwrap_or_default();
                let path = std::env::join_paths(
                    [r.join("bin"), r.join("lib")]
                        .into_iter()
                        .chain(std::env::split_paths(&old)),
                )
                .expect("PATH entries never contain the separator");
                c.env("PATH", path).env("YOSYSHQ_ROOT", r);
                c
            }
            None => Command::new(tool),
        }
    }

    /// Yosys's data directory (holds `ice40/cells_sim.v`): `<root>/share/yosys`,
    /// else `yosys-config --datdir`, else `<dir of yosys>/../share/yosys`.
    pub fn yosys_datdir(&self) -> Option<PathBuf> {
        let ok = |d: PathBuf| d.join("ice40").join("cells_sim.v").is_file().then_some(d);
        if let Some(r) = &self.root {
            return ok(r.join("share").join("yosys"));
        }
        let from_config = self
            .command("yosys-config")
            .arg("--datdir")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
        from_config.and_then(ok).or_else(|| {
            self.find("yosys")
                .and_then(|y| ok(y.parent()?.parent()?.join("share").join("yosys")))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A fresh, empty directory unique to this process and test.
    fn scratch(test: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mimz_toolchain_{}_{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"").unwrap();
    }

    #[test]
    fn a_suite_root_resolves_tools_under_its_bin() {
        let root = scratch("find");
        let yosys = root
            .join("bin")
            .join(format!("yosys{}", std::env::consts::EXE_SUFFIX));
        touch(&yosys);
        let tc = Toolchain {
            root: Some(root.clone()),
        };
        assert_eq!(tc.find("yosys"), Some(yosys));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_suite_root_puts_bin_and_lib_first_on_the_child_path() {
        let root = scratch("path");
        let tc = Toolchain {
            root: Some(root.clone()),
        };
        let cmd = tc.command("yosys");
        let env = |k: &str| {
            cmd.get_envs()
                .find(|(n, _)| *n == k)
                .and_then(|(_, v)| v)
                .map(|v| v.to_os_string())
        };
        let path = env("PATH").expect("PATH set on the child");
        let first: Vec<PathBuf> = std::env::split_paths(&path).take(2).collect();
        assert_eq!(first, vec![root.join("bin"), root.join("lib")]);
        assert_eq!(env("YOSYSHQ_ROOT"), Some(root.clone().into_os_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_tool_is_none() {
        let root = scratch("missing");
        let tc = Toolchain {
            root: Some(root.clone()),
        };
        assert_eq!(tc.find("yosys"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_suite_datdir_is_share_yosys() {
        let root = scratch("datdir");
        touch(&root.join("share/yosys/ice40/cells_sim.v"));
        let tc = Toolchain {
            root: Some(root.clone()),
        };
        assert_eq!(tc.yosys_datdir(), Some(root.join("share").join("yosys")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
