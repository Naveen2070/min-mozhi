//! Integration: the lab's content gate (site plan, work item W6).
//!
//! Every lab lesson under `site/content/lab/*.md` carries up to three tagged
//! code fences per step (`starter`, `solution`, `verify`). This test compiles
//! them with the real pipeline and proves each exercise is BOTH solvable and
//! not already solved:
//!
//! 1. `solution` passes `check`;
//! 2. `solution + verify` PASSES `test` — the exercise can be solved;
//! 3. `starter + verify` FAILS `test` — otherwise the grader is vacuous and
//!    passes a learner who did nothing;
//! 4. a starter annotated `​```mimz starter fails E0502` must fail `check`
//!    with EXACTLY that diagnostic code — "it errors somewhere" is not enough;
//! 5. every `/learn|/guide|/handbook` link (frontmatter `chapter:` included)
//!    resolves to a real page.
//!
//! Without this gate the lessons rot silently as the language moves; with it,
//! content drift is a build failure. Same doctrine as docs_sync/grammar_sync:
//! compile the snippet, then ship the sentence.

use std::fs;
use std::path::{Path, PathBuf};

use mimz_sim::run_command;

fn lab_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("site/content/lab")
}

/// One `## Step N` section's extracted machinery.
#[derive(Default, Clone)]
struct Step {
    n: u32,
    /// Diagnostic code the starter is SUPPOSED to fail `check` with
    /// (fence tag: ```mimz starter fails E0502).
    starter_fails: Option<String>,
    starter: Option<String>,
    solution: Option<String>,
    verify: Option<String>,
}

struct Lesson {
    chapter: Option<String>,
    module: Option<String>,
    steps: Vec<Step>,
}

/// Minimal line parser mirroring `site/src/lib/lab.ts` (no regex dependency).
/// Fence state is global, so a display block containing markdown cannot invent
/// structure; role fences (`​```mimz starter|solution|verify`) are captured
/// separately from ordinary display fences.
fn parse_lesson(text: &str) -> Lesson {
    let mut lesson = Lesson {
        chapter: None,
        module: None,
        steps: Vec::new(),
    };

    // ---- frontmatter ----
    let mut lines = text.lines().peekable();
    if lines.peek().is_some_and(|l| l.trim() == "---") {
        lines.next();
        for line in lines.by_ref() {
            let t = line.trim();
            if t == "---" {
                break;
            }
            if let Some(v) = t.strip_prefix("chapter:") {
                lesson.chapter = Some(v.trim().to_string());
            } else if let Some(v) = t.strip_prefix("module:") {
                lesson.module = Some(v.trim().to_string());
            }
        }
    }

    // ---- body ----
    let mut in_fence = false;
    // (step index, role) while a role fence's CONTENT is being read.
    let mut capture: Option<(usize, &'static str)> = None;

    for raw in lines {
        let line = raw.trim_end();
        let trimmed = line.trim_start();

        if let Some((idx, role)) = capture {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                capture = None; // closing fence
            } else if let Some(step) = lesson.steps.get_mut(idx) {
                let slot = match role {
                    "starter" => step.starter.get_or_insert_with(String::new),
                    "solution" => step.solution.get_or_insert_with(String::new),
                    "verify" => step.verify.get_or_insert_with(String::new),
                    _ => unreachable!("only known roles are captured"),
                };
                if !slot.is_empty() {
                    slot.push('\n');
                }
                slot.push_str(line);
            }
            continue;
        }

        if !in_fence {
            // A role fence opens a capture (it does NOT touch in_fence: its
            // content may contain anything, closed by its own fence mark).
            if trimmed.starts_with("```mimz ") {
                if let Some(idx) = lesson.steps.len().checked_sub(1) {
                    if let Some(role) = role_of(trimmed, idx, &mut lesson) {
                        capture = Some((idx, role));
                        continue;
                    }
                }
            }
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fence = true;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("## Step ") {
                let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = n.parse::<u32>() {
                    lesson.steps.push(Step {
                        n,
                        ..Step::default()
                    });
                }
            }
        }
        // Inside a display fence: only its own closing mark matters.
        else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = false;
        }
    }

    for s in &mut lesson.steps {
        for t in [&mut s.starter, &mut s.solution, &mut s.verify]
            .into_iter()
            .flatten()
        {
            *t = t.trim().to_string();
        }
    }
    lesson
}

/// Classify a ```` ```mimz … ```` opening line; records the optional
/// `fails E####` tag when the role is `starter`.
fn role_of(line: &str, idx: usize, lesson: &mut Lesson) -> Option<&'static str> {
    let tokens = line["```mimz ".len()..].split_whitespace();
    let mut role: Option<&'static str> = None;
    let mut fails: Option<String> = None;
    let mut expect_fails = false;
    for tok in tokens {
        match (role, tok) {
            (None, "starter") => role = Some("starter"),
            (None, "solution") => role = Some("solution"),
            (None, "verify") => role = Some("verify"),
            (Some(_), "fails") if role == Some("starter") => expect_fails = true,
            (Some(_), code) if expect_fails => {
                fails = Some(code.to_string());
                expect_fails = false;
            }
            _ => {}
        }
    }
    if let Some(step) = lesson.steps.get_mut(idx) {
        if role == Some("starter") {
            step.starter_fails = fails;
        }
    }
    role
}

/// Resolve a site route (`/learn/04-x`, `/guide/y`, `/handbook`) to a real
/// source file, so the link check tracks pages that actually exist.
fn route_resolves(base: &Path, route: &str) -> bool {
    let trimmed = route.trim_start_matches('/');
    let (section, rest) = match trimmed.split_once('/') {
        Some(pair) => pair,
        None => (trimmed, ""),
    };
    let root = match section {
        "learn" => base.join("site/content/learn"),
        "handbook" => base.join("site/content/handbook"),
        "guide" => base.join("docs/guide"),
        _ => return false,
    };
    if rest.is_empty() {
        return root.join("README.md").is_file();
    }
    root.join(format!("{rest}.md")).is_file() || root.join(rest).join("README.md").is_file()
}

/// Every `/learn|/guide|/handbook/…` path appearing anywhere in the file —
/// covers markdown links AND the frontmatter `chapter:` value.
fn linked_routes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for section in ["learn", "guide", "handbook"] {
        let needle = format!("/{section}/");
        let mut from = 0;
        while let Some(i) = text[from..].find(&needle) {
            let start = from + i;
            let end = start
                + text[start..]
                    .find(|c: char| c.is_whitespace() || c == ')' || c == '"')
                    .unwrap_or(text.len() - start);
            out.push(text[start..end].to_string());
            from = end;
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Module names a verify block binds to: everything after `for ` on its
/// `test "…" for Name` lines (parameter bindings `Name(W: 8)` included).
fn verify_targets(verify: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in verify.lines() {
        let t = line.trim_start();
        if !t.starts_with("test ") {
            continue;
        }
        if let Some(i) = t.find(" for ") {
            let rest = &t[i + 5..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn lab_lessons_are_solvable_and_not_pre_solved() {
    let dir = lab_dir();
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("site/content/lab must exist")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| p.file_name().is_some_and(|n| n != "README.md"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no lab lessons found in {}",
        dir.display()
    );

    let mut problems: Vec<String> = Vec::new();

    for file in &files {
        let id = file.file_stem().unwrap().to_string_lossy().to_string();
        let text = fs::read_to_string(file).unwrap();
        let lesson = parse_lesson(&text);

        if lesson.steps.is_empty() {
            problems.push(format!("{id}: no `## Step N` sections parsed"));
            continue;
        }

        // 5. Link targets resolve.
        if let Some(ch) = &lesson.chapter {
            if !route_resolves(base, ch) {
                problems.push(format!("{id}: chapter link `{ch}` does not resolve"));
            }
        }
        for r in linked_routes(&text) {
            if !route_resolves(base, &r) {
                problems.push(format!("{id}: link `{r}` does not resolve"));
            }
        }

        for s in &lesson.steps {
            let label = format!("{id} step {}", s.n);

            // 1. solution passes check.
            let Some(solution) = &s.solution else {
                problems.push(format!("{label}: missing solution fence"));
                continue;
            };
            if let Err(e) = run_command(solution, "check", &[]) {
                problems.push(format!("{label}: solution fails check:\n{e}"));
                continue;
            }

            let Some(verify) = &s.verify else {
                // Exploration step: graded on `check` alone, so its starter
                // must compile unless annotated as intentionally broken.
                match (&s.starter, &s.starter_fails) {
                    (None, _) => problems.push(format!("{label}: missing starter fence")),
                    (Some(starter), None) => {
                        if let Err(e) = run_command(starter, "check", &[]) {
                            problems.push(format!("{label}: starter does not compile:\n{e}"));
                        }
                    }
                    (Some(starter), Some(code)) => match run_command(starter, "check", &[]) {
                        Ok(_) => problems.push(format!(
                            "{label}: annotated fails {code} but the starter compiles"
                        )),
                        Err(e) if !e.contains(code) => {
                            problems.push(format!("{label}: expected diagnostic {code}, got:\n{e}"))
                        }
                        Err(_) => {}
                    },
                }
                continue;
            };

            // F4: whatever module a verify targets must be declared by the
            // step's own solution — renaming breaks the hidden contract.
            for target in verify_targets(verify) {
                if !solution.contains(&format!("module {target}")) {
                    problems.push(format!(
                        "{label}: verify targets module `{target}` but the solution never declares it"
                    ));
                }
            }

            // 2. solution + verify passes test — solvable.
            let solved = format!("{solution}\n\n{verify}");
            if let Err(e) = run_command(&solved, "test", &[]) {
                problems.push(format!(
                    "{label}: solution+verify FAILS test (unsolvable):\n{e}"
                ));
            }

            // 3. starter + verify must NOT pass — not pre-solved.
            let Some(starter) = &s.starter else {
                problems.push(format!("{label}: missing starter fence"));
                continue;
            };
            let attempted = format!("{starter}\n\n{verify}");
            match run_command(&attempted, "test", &[]) {
                Ok(_) => problems.push(format!(
                    "{label}: starter+verify PASSES test — vacuous check, grades nothing"
                )),
                Err(e) => {
                    if let Some(code) = &s.starter_fails {
                        if !e.contains(code) {
                            problems.push(format!(
                                "{label}: starter+verify failed with the wrong diagnostic (wanted {code}):\n{e}"
                            ));
                        }
                    }
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "lab content gate failed ({} problem{}):\n\n{}\n",
        problems.len(),
        if problems.len() == 1 { "" } else { "s" },
        problems.join("\n\n")
    );
}
