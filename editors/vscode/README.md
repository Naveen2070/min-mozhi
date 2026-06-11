# Min-Mozhi for VS Code

Syntax highlighting for `.mimz` files — all three keyword flavors
(English, Tanglish, Tamil script) highlight identically, including mixed
files, because the grammar lists every spelling from `keywords.toml`
(the repo's `tests/grammar_sync.rs` keeps them in lockstep).

## Install (from this folder, no marketplace yet)

Copy the folder into your VS Code extensions directory, then **close all
VS Code windows and reopen** (extensions are discovered at startup — a
window reload is not always enough):

```text
# Windows — remove any old copy first: if the destination folder already
# exists, Copy-Item nests the new copy INSIDE it instead of replacing it
Remove-Item -Recurse -Force "$env:USERPROFILE\.vscode\extensions\min-mozhi.mimz-0.1.0" -ErrorAction SilentlyContinue
Copy-Item -Recurse editors/vscode "$env:USERPROFILE\.vscode\extensions\min-mozhi.mimz-0.1.0"

# macOS / Linux
rm -rf ~/.vscode/extensions/min-mozhi.mimz-0.1.0
cp -r editors/vscode ~/.vscode/extensions/min-mozhi.mimz-0.1.0
```

After restarting, a `.mimz` file should show **Min-Mozhi** in the
status-bar language indicator (bottom right). If it still says Plain
Text, click the indicator → "Configure File Association for '.mimz'" →
Min-Mozhi.

Or — **recommended** — package it properly with
[`vsce`](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)
and install via the CLI (it prints an explicit success/failure, unlike
the GUI's "Install from VSIX"):

```text
cd editors/vscode
npx @vscode/vsce package   # produces mimz-0.1.0.vsix
code --install-extension mimz-0.1.0.vsix
```

**Troubleshooting — VSIX installs but the extension is invisible** (not
in the Extensions tab, not in the language list): if the extension was
ever folder-installed and that folder was later deleted, VS Code's
`~/.vscode/extensions/extensions.json` keeps a dangling entry. VS Code
then thinks the version is already installed and silently discards the
VSIX, while having nothing to load. Fix (with all VS Code windows
closed):

```text
code --uninstall-extension min-mozhi.mimz   # cleans the dangling entry
code --install-extension mimz-0.1.0.vsix --force
code --list-extensions                      # must list min-mozhi.mimz
```

**Troubleshooting — `code --list-extensions` shows it but the GUI
doesn't**: extensions are **per-profile**. If you use a custom VS Code
profile (check the gear icon → Profiles), the plain CLI installs into
the _Default_ profile only. Install into the profile you actually use:

```text
code --profile "YourProfileName" --install-extension mimz-0.1.0.vsix --force
```

Then exit VS Code completely (File → Exit, not just closing the window)
and reopen.

## What gets highlighted

- Declaration keywords (`module`/`thoguthi`/`தொகுதி`, `reg`/`nilai`/`நிலை`, …) — all flavors, plus the `include` alias of `import`
- Control keywords (`if`/`endral`/`என்றால்`, `match`/`poruthu`/`பொருத்து`, …)
- Module and enum names at their declaration
- Types (`bit`, `bits[…]`, `signed[…]`) and builtins (`extend`, `trunc`, …)
- Numbers with bases and `_` separators; test-name strings; both comment forms
- `<-` and the wrapping operators `+%`/`-%`/`*%` as distinct operator scopes
- Reserved words (`fall`, `struct`, `mem`, …) as invalid — they error in the compiler too

## Keeping it in sync

The keyword table is data (`keywords.toml`). When a spelling changes
there, `cargo test` fails (`tests/grammar_sync.rs`) until this grammar
is updated to match.
