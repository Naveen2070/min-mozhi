// Min-Mozhi VS Code extension: starts `mimz lsp` (diagnostics-only v0)
// for .mimz documents. Plain JavaScript on purpose — no build step, the
// file in the repo IS the file in the .vsix.

const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const serverPath = vscode.workspace
    .getConfiguration("mimz")
    .get("serverPath", "mimz");

  const serverOptions = {
    command: serverPath,
    args: ["lsp"],
  };
  const clientOptions = {
    documentSelector: [{ language: "mimz" }],
  };

  client = new LanguageClient(
    "mimz",
    "Min-Mozhi Language Server",
    serverOptions,
    clientOptions,
  );

  client.start().catch((err) => {
    vscode.window.showWarningMessage(
      `Min-Mozhi: could not start \`${serverPath} lsp\` (${err.message}). ` +
        "Syntax highlighting still works; install the mimz compiler or set " +
        "`mimz.serverPath` for live diagnostics.",
    );
  });
  context.subscriptions.push({ dispose: () => client && client.stop() });
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
