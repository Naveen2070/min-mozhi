//! LSP smoke test: spawn the REAL `mimz lsp` binary, speak framed JSON-RPC
//! over its stdio, and assert that opening a broken document produces a
//! `publishDiagnostics` notification carrying the stable E-code. This is
//! the editor experience, end to end — no tower-lsp test harness, just
//! the wire protocol an actual client uses.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn mimz_lsp() -> Child {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mimz lsp")
}

/// Write one framed JSON-RPC message.
fn send(to: &mut impl Write, body: &serde_json::Value) {
    let s = body.to_string();
    write!(to, "Content-Length: {}\r\n\r\n{s}", s.len()).unwrap();
    to.flush().unwrap();
}

/// Read framed messages on a thread, forwarding parsed JSON values.
fn reader_thread(out: impl Read + Send + 'static) -> mpsc::Receiver<serde_json::Value> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut r = BufReader::new(out);
        loop {
            let mut len: Option<usize> = None;
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    return; // server exited
                }
                let line = line.trim_end();
                if line.is_empty() {
                    break; // end of headers
                }
                if let Some(v) = line.strip_prefix("Content-Length:") {
                    len = v.trim().parse().ok();
                }
            }
            let Some(len) = len else { return };
            let mut body = vec![0u8; len];
            if r.read_exact(&mut body).is_err() {
                return;
            }
            let Ok(v) = serde_json::from_slice(&body) else {
                return;
            };
            if tx.send(v).is_err() {
                return;
            }
        }
    });
    rx
}

/// A `file://` URI for a path (test-grade: forward slashes, no escaping —
/// the temp dir has no spaces).
fn file_uri(path: &std::path::Path) -> String {
    format!("file:///{}", path.display().to_string().replace('\\', "/"))
}

#[test]
fn opening_a_broken_file_publishes_coded_diagnostics() {
    // The document is broken (unknown name `nope` → E0101). The text
    // travels IN-MEMORY via didOpen; the file itself is never written.
    let path = std::env::temp_dir().join("mimz_lsp_smoke.mimz");
    let uri = file_uri(&path);
    let text = "module M {\n  out y: bit\n  y = nope\n}\n";

    let mut child = mimz_lsp();
    let mut stdin = child.stdin.take().unwrap();
    let rx = reader_thread(child.stdout.take().unwrap());

    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "capabilities": {} }
        }),
    );
    let init = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("initialize response");
    assert_eq!(init["id"], 1);
    assert!(
        init["result"]["capabilities"]["textDocumentSync"].is_number(),
        "full-sync capability advertised: {init}"
    );

    send(
        &mut stdin,
        &serde_json::json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    );
    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "mimz", "version": 1, "text": text
            }}
        }),
    );

    // Skim notifications until the diagnostics arrive.
    let diags = loop {
        let msg = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("publishDiagnostics before timeout");
        if msg["method"] == "textDocument/publishDiagnostics" {
            break msg["params"].clone();
        }
    };
    let list = diags["diagnostics"].as_array().expect("diagnostics array");
    assert!(!list.is_empty(), "the broken file must produce diagnostics");
    assert_eq!(list[0]["code"], "E0101", "the stable E-code rides the wire");
    assert_eq!(list[0]["source"], "mimz");
    assert!(
        list[0]["message"].as_str().unwrap().contains("help:"),
        "the teaching help line is part of the message"
    );
    assert_eq!(
        list[0]["range"]["start"]["line"], 2,
        "0-based line of `nope`"
    );

    let _ = child.kill();
    let _ = child.wait();
}
