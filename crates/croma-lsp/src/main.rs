//! The `croma-lsp` binary: a thin, synchronous `lsp-server` stdio loop that owns
//! the document store and dispatches each request/notification to the
//! transport-free analysis layer in [`croma_lsp`].
//!
//! No business logic lives here (spec: "no business logic in the transport").
//! Every handler is wrapped so that a malformed or garbage message can never
//! panic the loop — diagnostics are computed from clamped, total functions and
//! decode failures are logged and skipped. There is no `unwrap`/`expect`/
//! `panic!`/indexing-that-panics/`debug_assert!` anywhere in this file.

use std::error::Error;

use croma_lsp::position::PositionEncoding;
use croma_lsp::{
    DocumentStore, diagnostics, document_symbols, folding_ranges, formatting, legend,
    semantic_tokens,
};
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    Notification as NotificationTrait, PublishDiagnostics,
};
use lsp_types::request::{
    DocumentSymbolRequest, FoldingRangeRequest, Formatting, Request as RequestTrait,
    SemanticTokensFullRequest,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRangeParams,
    InitializeParams, InitializeResult, OneOf, PositionEncodingKind, PublishDiagnosticsParams,
    SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, TextDocumentIdentifier,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();
    run(&connection)?;
    io_threads.join()?;
    Ok(())
}

/// Negotiate capabilities, finish initialization, then run the dispatch loop
/// until shutdown. Separated from `main` so the in-process transport test can
/// drive it over a `Connection::memory()` pair.
pub fn run(connection: &Connection) -> Result<(), Box<dyn Error + Sync + Send>> {
    let (id, params) = connection.initialize_start()?;
    let encoding = negotiate_encoding(&params);

    let capabilities = ServerCapabilities {
        position_encoding: Some(encoding.to_kind()),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        // R2: formatting (whole-doc replace), full semantic tokens (the legend
        // is the SAME one the analysis layer emits indices against), document
        // symbols, and folding ranges.
        document_formatting_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: Some(false),
                ..Default::default()
            },
        )),
        document_symbol_provider: Some(OneOf::Left(true)),
        folding_range_provider: Some(lsp_types::FoldingRangeProviderCapability::Simple(true)),
        ..Default::default()
    };
    let result = InitializeResult {
        capabilities,
        server_info: Some(ServerInfo {
            name: "croma-lsp".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };
    connection.initialize_finish(id, serde_json::to_value(result)?)?;

    main_loop(connection, encoding);
    Ok(())
}

/// Choose the position encoding: prefer UTF-8 if the client advertises it in
/// `general.position_encodings` (LSP 3.17), else fall back to the protocol
/// default UTF-16.
fn negotiate_encoding(params: &serde_json::Value) -> PositionEncoding {
    // Parse loosely: a malformed `initialize` must not abort negotiation.
    let offered = serde_json::from_value::<InitializeParams>(params.clone())
        .ok()
        .and_then(|p| p.capabilities.general)
        .and_then(|g| g.position_encodings);
    match offered {
        Some(kinds) if kinds.contains(&PositionEncodingKind::UTF8) => PositionEncoding::Utf8,
        _ => PositionEncoding::Utf16,
    }
}

/// The synchronous dispatch loop. Each message is handled in isolation; an
/// unrecognised method or a decode failure is ignored rather than fatal, so the
/// loop is total over arbitrary client traffic.
fn main_loop(connection: &Connection, encoding: PositionEncoding) {
    let mut store = DocumentStore::new();

    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                // The shutdown/exit dance is handled by lsp-server: it replies
                // to `shutdown` and tells us to stop once `exit` arrives.
                match connection.handle_shutdown(&request) {
                    Ok(true) => break,
                    Ok(false) => {
                        // Dispatch to an R2 request handler, or reply `null` for
                        // anything unsupported so the client never hangs.
                        let response = handle_request(&store, encoding, request);
                        send_response(connection, response);
                    }
                    Err(error) => {
                        log(format!("shutdown handling error: {error}"));
                        break;
                    }
                }
            }
            Message::Notification(notification) => {
                handle_notification(connection, &mut store, encoding, notification);
            }
            Message::Response(_) => {
                // The server issues no client-bound requests in R1, so any
                // response is unexpected; ignore it rather than panic.
            }
        }
    }
}

/// Dispatch a (non-shutdown) request to the matching R2 analysis function over
/// the stored document text, producing a [`Response`].
///
/// Total: an unrecognised method, a payload that fails to decode, an unopened
/// document, or a serialisation failure all yield a `null`-result response
/// rather than an error response or a panic, so a conformant client is never
/// left hanging and the loop never aborts. The analysis functions themselves are
/// panic-free (they parse/format clamped, total input).
fn handle_request(store: &DocumentStore, encoding: PositionEncoding, request: Request) -> Response {
    let id = request.id.clone();
    match request.method.as_str() {
        Formatting::METHOD => {
            let Some(uri) = formatting_uri(&request) else {
                return null_ok(id);
            };
            let text = store.get(&uri).unwrap_or("");
            ok_or_null(id, &formatting(text, encoding))
        }
        SemanticTokensFullRequest::METHOD => {
            let Some(uri) = semantic_tokens_uri(&request) else {
                return null_ok(id);
            };
            let text = store.get(&uri).unwrap_or("");
            ok_or_null(id, &semantic_tokens(text, encoding))
        }
        DocumentSymbolRequest::METHOD => {
            let Some(uri) = document_symbol_uri(&request) else {
                return null_ok(id);
            };
            let text = store.get(&uri).unwrap_or("");
            ok_or_null(
                id,
                &DocumentSymbolResponse::Nested(document_symbols(text, encoding)),
            )
        }
        FoldingRangeRequest::METHOD => {
            let Some(uri) = folding_range_uri(&request) else {
                return null_ok(id);
            };
            let text = store.get(&uri).unwrap_or("");
            ok_or_null(id, &folding_ranges(text, encoding))
        }
        // Any other request (e.g. hover in R1) — reply null, never hang.
        _ => null_ok(id),
    }
}

/// Decode the `textDocument.uri` of each request kind, logging + dropping a
/// malformed payload. Kept per-kind so each uses its real `Params` type.
fn formatting_uri(request: &Request) -> Option<Url> {
    decode_uri::<DocumentFormattingParams>(request, |p| p.text_document)
}
fn semantic_tokens_uri(request: &Request) -> Option<Url> {
    decode_uri::<SemanticTokensParams>(request, |p| p.text_document)
}
fn document_symbol_uri(request: &Request) -> Option<Url> {
    decode_uri::<DocumentSymbolParams>(request, |p| p.text_document)
}
fn folding_range_uri(request: &Request) -> Option<Url> {
    decode_uri::<FoldingRangeParams>(request, |p| p.text_document)
}

/// Decode a request's params and extract its document identifier's URI.
fn decode_uri<P>(request: &Request, pick: impl FnOnce(P) -> TextDocumentIdentifier) -> Option<Url>
where
    P: serde::de::DeserializeOwned,
{
    match serde_json::from_value::<P>(request.params.clone()) {
        Ok(params) => Some(pick(params).uri),
        Err(error) => {
            log(format!("{} decode error: {error}", request.method));
            None
        }
    }
}

/// Serialise `value` into an ok-response, falling back to `null` on failure.
fn ok_or_null<T: serde::Serialize>(id: RequestId, value: &T) -> Response {
    match serde_json::to_value(value) {
        Ok(value) => Response::new_ok(id, value),
        Err(error) => {
            log(format!("failed to serialise response: {error}"));
            null_ok(id)
        }
    }
}

/// A `null`-result ok-response.
fn null_ok(id: RequestId) -> Response {
    Response::new_ok(id, serde_json::Value::Null)
}

/// Send a response, logging a transport failure.
fn send_response(connection: &Connection, response: Response) {
    if let Err(error) = connection.sender.send(Message::Response(response)) {
        log(format!("failed to send response: {error}"));
    }
}

/// Route a notification to the document store and (re)publish diagnostics. Each
/// arm decodes defensively: a payload that fails to parse is logged and dropped.
fn handle_notification(
    connection: &Connection,
    store: &mut DocumentStore,
    encoding: PositionEncoding,
    notification: Notification,
) {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            match cast_notification::<DidOpenTextDocument>(notification) {
                Ok(params) => {
                    let DidOpenTextDocumentParams { text_document } = params;
                    let uri = text_document.uri.clone();
                    store.open(uri.clone(), text_document.text);
                    publish(connection, store, encoding, &uri);
                }
                Err(error) => log(format!("didOpen decode error: {error}")),
            }
        }
        DidChangeTextDocument::METHOD => {
            match cast_notification::<DidChangeTextDocument>(notification) {
                Ok(params) => {
                    let DidChangeTextDocumentParams {
                        text_document,
                        content_changes,
                    } = params;
                    let uri = text_document.uri.clone();
                    store.change(&uri, content_changes, encoding);
                    publish(connection, store, encoding, &uri);
                }
                Err(error) => log(format!("didChange decode error: {error}")),
            }
        }
        DidCloseTextDocument::METHOD => {
            match cast_notification::<DidCloseTextDocument>(notification) {
                Ok(params) => {
                    let DidCloseTextDocumentParams { text_document } = params;
                    store.close(&text_document.uri);
                    // Clearing diagnostics on close is conventional.
                    publish_diagnostics(connection, &text_document.uri, Vec::new());
                }
                Err(error) => log(format!("didClose decode error: {error}")),
            }
        }
        // `initialized`, `$/setTrace`, `$/cancelRequest`, etc. — nothing to do.
        _ => {}
    }
}

/// Compute and publish diagnostics for `uri` from its current stored text.
fn publish(connection: &Connection, store: &DocumentStore, encoding: PositionEncoding, uri: &Url) {
    let text = store.get(uri).unwrap_or("");
    let diags = diagnostics(text, encoding);
    publish_diagnostics(connection, uri, diags);
}

/// Send a `textDocument/publishDiagnostics` notification.
fn publish_diagnostics(connection: &Connection, uri: &Url, diags: Vec<lsp_types::Diagnostic>) {
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: diags,
        version: None,
    };
    let value = match serde_json::to_value(params) {
        Ok(value) => value,
        Err(error) => {
            log(format!("failed to serialise diagnostics: {error}"));
            return;
        }
    };
    let notification = Notification {
        method: PublishDiagnostics::METHOD.to_string(),
        params: value,
    };
    if let Err(error) = connection.sender.send(Message::Notification(notification)) {
        log(format!("failed to publish diagnostics: {error}"));
    }
}

/// Decode a notification's params into `N::Params`, mapping a method mismatch
/// into an error string (it should not happen — we match on the method first).
fn cast_notification<N>(notification: Notification) -> Result<N::Params, String>
where
    N: NotificationTrait,
    N::Params: serde::de::DeserializeOwned,
{
    notification
        .extract::<N::Params>(N::METHOD)
        .map_err(|error| match error {
            ExtractError::MethodMismatch(n) => format!("method mismatch: {}", n.method),
            ExtractError::JsonError { method, error } => format!("{method}: {error}"),
        })
}

/// Minimal diagnostic logging to stderr (stdout is the LSP channel). Kept as a
/// single sink so the loop never uses `eprintln!` ad hoc.
fn log(message: String) {
    eprintln!("croma-lsp: {message}");
}

#[cfg(test)]
mod transport_tests {
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
        TextDocumentIdentifier, TextDocumentItem, Url, VersionedTextDocumentIdentifier,
    };

    use super::run;

    const RECV_TIMEOUT: Duration = Duration::from_secs(10);

    fn uri() -> Url {
        Url::parse("file:///scripted.abc").expect("valid uri")
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
        let (server, client) = Connection::memory();

        // Run the real server loop in its own thread.
        let server_thread = thread::spawn(move || run(&server));

        // 1. initialize (offer UTF-8 so the server negotiates it).
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

        // Expect the InitializeResult response.
        match recv(&client) {
            Message::Response(Response { id, result, error }) => {
                assert_eq!(id, init_id);
                assert!(error.is_none(), "initialize errored: {error:?}");
                assert!(result.is_some(), "initialize returned no result");
            }
            other => panic!("expected initialize response, got {other:?}"),
        }

        // 2. initialized.
        notify(&client, "initialized", serde_json::json!({}));

        // 3. didOpen a real-ish ABC document.
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

        // 4. several didChange notifications, including a malformed/garbage edit.
        // 4a. incremental insert.
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

        // 4b. a garbage didChange payload — must not crash the loop.
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

        // 4b.5. Drive each R2 request over the (now malformed mid-edit) buffer
        // and assert the server replies without hanging or panicking. The buffer
        // currently holds "[[[ broken é\n" from the previous garbage edit, so
        // these run against a deliberately broken state.
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
                Message::Response(Response { id, error, .. }) => {
                    assert_eq!(id, req_id, "{method} response id");
                    assert!(error.is_none(), "{method} errored: {error:?}");
                }
                other => panic!("expected {method} response, got {other:?}"),
            }
        }

        // 4b.6. Restore a real tune and re-run the requests to assert non-empty,
        // well-formed payloads (semantic tokens + symbols + folding present).
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

        // semanticTokens/full -> non-empty data.
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
            Message::Response(Response { id, result, error }) => {
                assert_eq!(id, st_id);
                assert!(error.is_none(), "semanticTokens errored: {error:?}");
                let tokens: lsp_types::SemanticTokens =
                    serde_json::from_value(result.expect("semanticTokens result"))
                        .expect("valid SemanticTokens");
                assert!(!tokens.data.is_empty(), "real tune should yield tokens");
            }
            other => panic!("expected semanticTokens response, got {other:?}"),
        }

        // formatting -> a single whole-document edit (the body had loose spaces).
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
            Message::Response(Response { id, result, error }) => {
                assert_eq!(id, fmt_id);
                assert!(error.is_none(), "formatting errored: {error:?}");
                let edits: Vec<lsp_types::TextEdit> =
                    serde_json::from_value(result.expect("formatting result"))
                        .expect("valid TextEdit list");
                assert!(edits.len() <= 1, "at most one full-document edit");
            }
            other => panic!("expected formatting response, got {other:?}"),
        }

        // documentSymbol -> one symbol for the tune.
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
            Message::Response(Response { id, result, error }) => {
                assert_eq!(id, sym_id);
                assert!(error.is_none(), "documentSymbol errored: {error:?}");
                let response: lsp_types::DocumentSymbolResponse =
                    serde_json::from_value(result.expect("documentSymbol result"))
                        .expect("valid DocumentSymbolResponse");
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

        // foldingRange -> one region fold over the tune.
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
            Message::Response(Response { id, result, error }) => {
                assert_eq!(id, fold_id);
                assert!(error.is_none(), "foldingRange errored: {error:?}");
                let folds: Vec<lsp_types::FoldingRange> =
                    serde_json::from_value(result.expect("foldingRange result"))
                        .expect("valid FoldingRange list");
                assert_eq!(folds.len(), 1, "one fold per tune");
            }
            other => panic!("expected foldingRange response, got {other:?}"),
        }

        // 4c. an entirely unknown request — server must reply (null), not hang.
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

        // 5. didClose -> publishes empty diagnostics.
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

        // 6. shutdown -> response; exit -> loop ends.
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
}
