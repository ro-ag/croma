//! Scripted-client transport test (promotion-bar leg C, transport half):
//! drive the real server loop over a `Connection::memory()` pair from a
//! spawned thread and assert it never hangs (every client `recv` is bounded
//! by `recv_timeout`) and the server thread joins cleanly (0 panics).

use std::thread;
use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, GeneralClientCapabilities, InitializeParams, Position,
    PositionEncodingKind, PublishDiagnosticsParams, Range, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};

use super::run;

const RECV_TIMEOUT: Duration = Duration::from_secs(10);

fn uri() -> Uri {
    <Uri as std::str::FromStr>::from_str("file:///scripted.abc").expect("valid uri")
}

/// Block on a client receive, failing the test on timeout so a hang is a
/// loud failure rather than a wedged test.
fn recv(client: &Connection) -> Message {
    client
        .receiver
        .recv_timeout(RECV_TIMEOUT)
        .expect("client receive timed out — the server loop hung")
}

fn notify(client: &Connection, method: &str, params: impl serde::Serialize) {
    let value = serde_json::to_value(params).expect("serialise notification params");
    client
        .sender
        .send(Message::Notification(Notification {
            method: method.to_string(),
            params: value,
        }))
        .expect("send notification");
}

/// Drain published diagnostics until the next message arrives that is not a
/// `publishDiagnostics`, returning how many we saw. Each receive is bounded.
fn drain_diagnostics(client: &Connection, budget: usize) -> usize {
    let mut seen = 0;
    for _ in 0..budget {
        match recv(client) {
            Message::Notification(n) if n.method == "textDocument/publishDiagnostics" => {
                // Confirm it deserialises and is well-formed.
                let parsed: PublishDiagnosticsParams =
                    serde_json::from_value(n.params).expect("valid publishDiagnostics");
                assert_eq!(parsed.uri, uri());
                seen += 1;
            }
            other => panic!("expected publishDiagnostics, got {other:?}"),
        }
    }
    seen
}

#[test]
fn scripted_client_drives_server_without_hanging_or_panicking() {
    // One scripted session walks the *entire* server lifecycle end to end —
    // handshake, edits, malformed input, every R2/R3 request, teardown — in a
    // single test on purpose: a real editor interleaves these, so ordering and
    // state-carry-over bugs only surface when the steps run against one another
    // in sequence. Two properties are under test throughout (leg C, transport
    // half): the loop NEVER hangs (every client `recv` is bounded by
    // `RECV_TIMEOUT`, so a wedged server fails loudly instead of blocking) and
    // it ends with 0 panics (the server thread joins cleanly at the very end).
    // The real `run` loop drives the server on its own thread; the test plays
    // the client over an in-memory connection.
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(move || run(&server));

    // Phase 1 — handshake. `initialize` (offering UTF-8 so the server
    // negotiates that encoding) must return an InitializeResult; then
    // `initialized`; then didOpen a real tune, which publishes diagnostics
    // exactly once.
    let init_params = InitializeParams {
        capabilities: ClientCapabilities {
            general: Some(GeneralClientCapabilities {
                position_encodings: Some(vec![PositionEncodingKind::UTF8]),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let init_id = RequestId::from(1);
    client
        .sender
        .send(Message::Request(Request {
            id: init_id.clone(),
            method: "initialize".to_string(),
            params: serde_json::to_value(init_params).expect("serialise initialize"),
        }))
        .expect("send initialize");

    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, init_id);
            response_result.expect("initialize errored");
        }
        other => panic!("expected initialize response, got {other:?}"),
    }

    notify(&client, "initialized", serde_json::json!({}));

    notify(
        &client,
        "textDocument/didOpen",
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri(),
                language_id: "abc".to_string(),
                version: 1,
                text: "X:1\nT:Scripted\nK:C\nCDEF|\n".to_string(),
            },
        },
    );
    assert_eq!(
        drain_diagnostics(&client, 1),
        1,
        "didOpen should publish once"
    );

    // Phase 2 — edits. A well-formed incremental (ranged) insert; the server
    // re-parses and re-publishes once.
    notify(
        &client,
        "textDocument/didChange",
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 3,
                        character: 4,
                    },
                    end: Position {
                        line: 3,
                        character: 4,
                    },
                }),
                range_length: None,
                text: "GABc|".to_string(),
            }],
        },
    );
    assert_eq!(drain_diagnostics(&client, 1), 1);

    // Phase 3 — malformed input the loop must shrug off. A didChange whose
    // payload has the wrong JSON shape fails to decode and is silently dropped
    // (no publish); we then prove the server is still alive by sending a valid
    // change and seeing diagnostics again. That change's text is itself broken
    // ABC ("[[[ broken é"), leaving the buffer deliberately malformed for the
    // next phase.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didChange".to_string(),
            params: serde_json::json!({ "totally": "wrong", "shape": [1, 2, 3] }),
        }))
        .expect("send garbage didChange");
    // No diagnostics expected (decode fails, dropped) — verify the server is
    // still alive by sending another valid change and getting diagnostics.
    notify(
        &client,
        "textDocument/didChange",
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 3,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "[[[ broken \u{e9}\n".to_string(),
            }],
        },
    );
    assert_eq!(
        drain_diagnostics(&client, 1),
        1,
        "server survived garbage edit"
    );

    // Phase 4 — every R2 request must REPLY (not hang, not panic) even over
    // the malformed buffer left by Phase 3. The handlers must degrade to an
    // empty/standard response on a broken parse rather than crash the loop.
    for (id_n, method) in [
        (10i32, "textDocument/formatting"),
        (11, "textDocument/semanticTokens/full"),
        (12, "textDocument/documentSymbol"),
        (13, "textDocument/foldingRange"),
    ] {
        let req_id = RequestId::from(id_n);
        client
            .sender
            .send(Message::Request(Request {
                id: req_id.clone(),
                method: method.to_string(),
                params: serde_json::json!({
                    "textDocument": { "uri": uri().to_string() }
                }),
            }))
            .expect("send R2 request");
        match recv(&client) {
            Message::Response(Response {
                id,
                response_result,
            }) => {
                assert_eq!(id, req_id, "{method} response id");
                if let Err(error) = response_result {
                    panic!("{method} errored: {error:?}");
                }
            }
            other => panic!("expected {method} response, got {other:?}"),
        }
    }

    // Phase 5 — the inverse of Phase 4: restore a real tune, re-run the same
    // handlers, and assert their payloads are now non-empty and well-formed
    // (real semantic tokens, one "Probe" symbol, one fold, a hover on `K:`).
    // Good input must produce real results, not just "didn't crash".
    notify(
        &client,
        "textDocument/didChange",
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 4,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "X:1\nT:Probe\nK:C\nC   D E F|\n".to_string(),
            }],
        },
    );
    assert_eq!(drain_diagnostics(&client, 1), 1, "restore published once");

    let st_id = RequestId::from(20);
    client
        .sender
        .send(Message::Request(Request {
            id: st_id.clone(),
            method: "textDocument/semanticTokens/full".to_string(),
            params: serde_json::json!({ "textDocument": { "uri": uri().to_string() } }),
        }))
        .expect("send semanticTokens");
    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, st_id);
            let result = response_result.expect("semanticTokens errored");
            let tokens: lsp_types::SemanticTokens =
                serde_json::from_value(result).expect("valid SemanticTokens");
            assert!(!tokens.data.is_empty(), "real tune should yield tokens");
        }
        other => panic!("expected semanticTokens response, got {other:?}"),
    }

    let fmt_id = RequestId::from(21);
    client
        .sender
        .send(Message::Request(Request {
            id: fmt_id.clone(),
            method: "textDocument/formatting".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri().to_string() },
                "options": { "tabSize": 2, "insertSpaces": true }
            }),
        }))
        .expect("send formatting");
    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, fmt_id);
            let result = response_result.expect("formatting errored");
            let edits: Vec<lsp_types::TextEdit> =
                serde_json::from_value(result).expect("valid TextEdit list");
            assert!(edits.len() <= 1, "at most one full-document edit");
        }
        other => panic!("expected formatting response, got {other:?}"),
    }

    let sym_id = RequestId::from(22);
    client
        .sender
        .send(Message::Request(Request {
            id: sym_id.clone(),
            method: "textDocument/documentSymbol".to_string(),
            params: serde_json::json!({ "textDocument": { "uri": uri().to_string() } }),
        }))
        .expect("send documentSymbol");
    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, sym_id);
            let result = response_result.expect("documentSymbol errored");
            let response: lsp_types::DocumentSymbolResponse =
                serde_json::from_value(result).expect("valid DocumentSymbolResponse");
            match response {
                lsp_types::DocumentSymbolResponse::Nested(symbols) => {
                    assert_eq!(symbols.len(), 1, "one tune symbol");
                    assert_eq!(symbols[0].name, "Probe");
                }
                other => panic!("expected nested symbols, got {other:?}"),
            }
        }
        other => panic!("expected documentSymbol response, got {other:?}"),
    }

    let fold_id = RequestId::from(23);
    client
        .sender
        .send(Message::Request(Request {
            id: fold_id.clone(),
            method: "textDocument/foldingRange".to_string(),
            params: serde_json::json!({ "textDocument": { "uri": uri().to_string() } }),
        }))
        .expect("send foldingRange");
    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, fold_id);
            let result = response_result.expect("foldingRange errored");
            let folds: Vec<lsp_types::FoldingRange> =
                serde_json::from_value(result).expect("valid FoldingRange list");
            assert_eq!(folds.len(), 1, "one fold per tune");
        }
        other => panic!("expected foldingRange response, got {other:?}"),
    }

    // Hover on the `K:` key line (line 2, col 0).
    let hover_id = RequestId::from(30);
    client
        .sender
        .send(Message::Request(Request {
            id: hover_id.clone(),
            method: "textDocument/hover".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri().to_string() },
                "position": { "line": 2, "character": 0 }
            }),
        }))
        .expect("send hover");
    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, hover_id);
            let result = response_result.expect("hover errored");
            let hover: Option<lsp_types::Hover> =
                serde_json::from_value(result).expect("valid Hover");
            assert!(hover.is_some(), "K: line should hover to the key doc");
        }
        other => panic!("expected hover response, got {other:?}"),
    }

    // Phase 6 — completion. Insert a blank line so line 1 is an empty header
    // context, then assert completion there offers a field key (`K:`).
    notify(
        &client,
        "textDocument/didChange",
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 5,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "X:1\n\nK:C\nCDEF|\n".to_string(),
            }],
        },
    );
    assert_eq!(drain_diagnostics(&client, 1), 1, "edit re-published");
    let comp_id = RequestId::from(31);
    client
        .sender
        .send(Message::Request(Request {
            id: comp_id.clone(),
            method: "textDocument/completion".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri().to_string() },
                "position": { "line": 1, "character": 0 }
            }),
        }))
        .expect("send completion");
    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, comp_id);
            let result = response_result.expect("completion errored");
            let items: Vec<lsp_types::CompletionItem> =
                serde_json::from_value(result).expect("valid CompletionItem list");
            assert!(
                items.iter().any(|i| i.label == "K:"),
                "header completion offers K:"
            );
        }
        other => panic!("expected completion response, got {other:?}"),
    }

    // Phase 7 — code action. A buffer with a fixable issue (`Q:320s`, a
    // bare-suffix tempo) must yield exactly one `source.fixAll` action.
    notify(
        &client,
        "textDocument/didChange",
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 6,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "X:1\nQ:320s\nK:C\nCDEF|\n".to_string(),
            }],
        },
    );
    assert_eq!(
        drain_diagnostics(&client, 1),
        1,
        "fixable edit re-published"
    );
    let ca_id = RequestId::from(32);
    client
        .sender
        .send(Message::Request(Request {
            id: ca_id.clone(),
            method: "textDocument/codeAction".to_string(),
            params: serde_json::json!({
                "textDocument": { "uri": uri().to_string() },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 0 }
                },
                "context": { "diagnostics": [] }
            }),
        }))
        .expect("send codeAction");
    match recv(&client) {
        Message::Response(Response {
            id,
            response_result,
        }) => {
            assert_eq!(id, ca_id);
            let result = response_result.expect("codeAction errored");
            let actions: Vec<lsp_types::CodeAction> =
                serde_json::from_value(result).expect("valid CodeAction list");
            assert_eq!(actions.len(), 1, "one fix-all action for Q:320s");
            assert_eq!(
                actions[0].kind,
                Some(lsp_types::CodeActionKind::SOURCE_FIX_ALL)
            );
        }
        other => panic!("expected codeAction response, got {other:?}"),
    }

    // Phase 8 — robustness: an odd/empty-params request (a hover with `{}`)
    // must still get a reply, never wedge the loop waiting for a response.
    let probe_id = RequestId::from(2);
    client
        .sender
        .send(Message::Request(Request {
            id: probe_id.clone(),
            method: "textDocument/hover".to_string(),
            params: serde_json::json!({}),
        }))
        .expect("send unknown request");
    match recv(&client) {
        Message::Response(Response { id, .. }) => assert_eq!(id, probe_id),
        other => panic!("expected a response to the unknown request, got {other:?}"),
    }

    // Phase 9 — teardown. didClose clears diagnostics (one final publish);
    // `shutdown` returns a response; `exit` ends the loop so the thread joins.
    notify(
        &client,
        "textDocument/didClose",
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri() },
        },
    );
    assert_eq!(
        drain_diagnostics(&client, 1),
        1,
        "didClose clears diagnostics"
    );

    let shutdown_id = RequestId::from(3);
    client
        .sender
        .send(Message::Request(Request {
            id: shutdown_id.clone(),
            method: "shutdown".to_string(),
            params: serde_json::Value::Null,
        }))
        .expect("send shutdown");
    match recv(&client) {
        Message::Response(Response { id, .. }) => assert_eq!(id, shutdown_id),
        other => panic!("expected shutdown response, got {other:?}"),
    }
    notify(&client, "exit", serde_json::Value::Null);

    // The server thread must join cleanly (0 panics, 0 hangs).
    let joined = server_thread.join();
    assert!(joined.is_ok(), "server thread panicked");
    let run_result = joined.expect("server thread joined");
    assert!(
        run_result.is_ok(),
        "server run returned error: {run_result:?}"
    );
}
