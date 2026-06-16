//! `mimz.toml` — per-project defaults for CLI flags, so a flag you set once for
//! a project need not be repeated every invocation.
//!
//! Precedence is **CLI flag › `mimz.toml` value › built-in default**: the config
//! only fills in what you did not pass on the command line. The file is
//! discovered by walking up from the input file to the nearest `mimz.toml` (like
//! `Cargo.toml` / `rustfmt.toml`); `--config <path>` overrides the search. No
//! file found ⇒ an all-default [`Config`]. A malformed file is a clean error —
//! unlike the EMBEDDED keyword tables (which panic at startup), this is
//! user-authored and per-project, so it must fail gracefully.
//!
//! Every field is `Option` so "set" is distinguishable from "absent"; the CLI
//! layer (`src/main.rs`) does the `cli.or(config).unwrap_or(default)` merge.
//! Format is TOML to match the project's other human-authored tables
//! (`keywords.toml`, `case_suffixes.toml`) — the machine-written name-map sidecar
//! stays JSON.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Max directories the `mimz.toml` walk-up will visit before giving up. Bounds
/// the directory-stat chain for a pathologically deep input path; far beyond any
/// real project nesting.
const MAX_CONFIG_WALK_DEPTH: usize = 256;

/// A parsed `mimz.toml`. All fields optional; `deny_unknown_fields` turns a
/// typo'd key (e.g. `[tranlsate]`) into an error rather than a silent no-op.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Default diagnostics language (`check`/`compile`/`eval`): english |
    /// tanglish | tamil. Overridden by `--lang`; itself overrides the file's
    /// keyword-majority default.
    pub lang: Option<String>,
    /// Defaults for `mimz translate`.
    #[serde(default)]
    pub translate: TranslateConfig,
    /// Defaults for `mimz fmt`.
    #[serde(default)]
    pub fmt: FmtConfig,
}

/// `[translate]` — defaults for the reskin / romanize subcommand.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TranslateConfig {
    /// Default target keyword flavor (`--to`): english | tanglish | tamil.
    pub to: Option<String>,
    /// Default word order (`--order`): code | thamizh.
    pub order: Option<String>,
    /// Romanize Tamil identifiers to Latin by default (`--romanize-names`).
    pub romanize_names: Option<bool>,
    /// Name-map auto-discovery on reverse translate: `"auto"` (load
    /// `<input>.names.json` if present) or `"off"`. Default is `"auto"`.
    pub names_map: Option<String>,
}

/// `[fmt]` — defaults for the in-place keyword-flavor normalizer.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FmtConfig {
    /// Default target flavor (`--to`); absent ⇒ the file's keyword majority.
    pub to: Option<String>,
    /// Warn-and-fail on a mixed-flavor file by default (`--strict`).
    pub strict: Option<bool>,
}

impl Config {
    /// Walk up from `start` (a file or directory) to the nearest `mimz.toml`.
    /// The start path is canonicalized first so a relative input still walks
    /// the real directory chain; `None` if no config exists up to the root.
    pub fn discover(start: &Path) -> Option<PathBuf> {
        let abs = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        let mut dir: Option<&Path> = if abs.is_dir() {
            Some(&abs)
        } else {
            abs.parent()
        };
        // Bound the walk-up so a pathologically deep input path can't trigger an
        // unbounded chain of directory stats (defensive; far past any real tree).
        for _ in 0..MAX_CONFIG_WALK_DEPTH {
            let Some(d) = dir else { break };
            let candidate = d.join("mimz.toml");
            if candidate.is_file() {
                return Some(candidate);
            }
            dir = d.parent();
        }
        None
    }

    /// Read and parse a config file. The `Err` is a ready-to-print message.
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read config `{}`: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("invalid config `{}`: {e}", path.display()))
    }

    /// Resolve the config governing `input`: an explicit `--config` path wins;
    /// otherwise discover by walking up from `input`; no file ⇒ all defaults.
    pub fn resolve(input: &Path, explicit: Option<&Path>) -> Result<Config, String> {
        match explicit {
            Some(p) => Config::load(p),
            None => match Config::discover(input) {
                Some(p) => Config::load(&p),
                None => Ok(Config::default()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_all_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c, Config::default());
        assert!(c.lang.is_none() && c.translate.to.is_none() && c.fmt.strict.is_none());
    }

    #[test]
    fn parses_every_section() {
        let src = r#"
            lang = "tamil"
            [translate]
            to = "tanglish"
            order = "code"
            romanize_names = true
            names_map = "off"
            [fmt]
            to = "tamil"
            strict = true
        "#;
        let c: Config = toml::from_str(src).unwrap();
        assert_eq!(c.lang.as_deref(), Some("tamil"));
        assert_eq!(c.translate.to.as_deref(), Some("tanglish"));
        assert_eq!(c.translate.order.as_deref(), Some("code"));
        assert_eq!(c.translate.romanize_names, Some(true));
        assert_eq!(c.translate.names_map.as_deref(), Some("off"));
        assert_eq!(c.fmt.to.as_deref(), Some("tamil"));
        assert_eq!(c.fmt.strict, Some(true));
    }

    #[test]
    fn unknown_key_is_rejected() {
        // A typo'd key must error, not be silently ignored.
        assert!(toml::from_str::<Config>("[translate]\ntoo = \"tamil\"\n").is_err());
        assert!(toml::from_str::<Config>("flavour = \"tamil\"\n").is_err());
    }

    #[test]
    fn discover_walks_up_to_the_nearest_config() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir().join(format!(
            "mimz_cfg_test_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let sub = base.join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(base.join("mimz.toml"), "lang = \"tamil\"\n").unwrap();
        let input = sub.join("thing.mimz");
        std::fs::write(&input, "module M {}\n").unwrap();

        let found = Config::discover(&input).expect("walks up to base/mimz.toml");
        assert_eq!(
            found,
            std::fs::canonicalize(base.join("mimz.toml")).unwrap()
        );
        assert_eq!(
            Config::resolve(&input, None).unwrap().lang.as_deref(),
            Some("tamil")
        );

        std::fs::remove_dir_all(&base).ok();
    }
}
