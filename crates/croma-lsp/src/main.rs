//! The `croma-lsp` binary: a thin, synchronous `lsp-server` stdio loop that owns
//! the document store and dispatches each request/notification to the
//! transport-free analysis layer in [`croma_lsp`].
//!
//! No business logic lives here. Decode failures are logged and skipped, and
//! every analysis call (requests and published diagnostics) runs under
//! [`contain`], so a panic in croma-core is logged and answered with a `null`
//! result or an internal-error diagnostic while the loop keeps serving. This
//! file itself has no `unwrap`/`expect`/`panic!` outside its tests.

use std::error::Error;

use croma_lsp::position::PositionEncoding;
use croma_lsp::{
    DocumentStore, code_actions, completion, contain, diagnostics, document_symbols,
    folding_ranges, formatting, hover, internal_error_diagnostic, legend, semantic_tokens,
};
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    Notification as NotificationTrait, PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentSymbolRequest, FoldingRangeRequest, Formatting,
    HoverRequest, Request as RequestTrait, SemanticTokensFullRequest,
};
use lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionParams, CodeActionProviderCapability,
    CompletionOptions, CompletionParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, FoldingRangeParams, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, OneOf, PositionEncodingKind, PublishDiagnosticsParams,
    SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, TextDocumentIdentifier,
    TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
    WorkDoneProgressOptions,
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
        // R3: hover (field/decoration docs), completion (header keys + decoration
        // names, triggered after `!`/`+`), and code actions (the `source.fixAll`
        // whole-document auto-fix; `quickfix` advertised so a client that filters
        // by kind still surfaces it).
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["!".to_string(), "+".to_string()]),
            ..Default::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![
                CodeActionKind::SOURCE_FIX_ALL,
                CodeActionKind::QUICKFIX,
            ]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
            resolve_provider: Some(false),
        })),
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
                        let id = request.id.clone();
                        let response = contain(|| handle_request(&store, encoding, request))
                            .unwrap_or_else(|| null_ok(id));
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
        HoverRequest::METHOD => {
            let Some((uri, position)) = hover_request(&request) else {
                return null_ok(id);
            };
            let text = store.get(&uri).unwrap_or("");
            // `hover` already returns `Option<Hover>`; `None` serialises to
            // `null`, which is the correct "no hover" reply.
            ok_or_null(id, &hover(text, position, encoding))
        }
        Completion::METHOD => {
            let Some((uri, position)) = completion_request(&request) else {
                return null_ok(id);
            };
            let text = store.get(&uri).unwrap_or("");
            ok_or_null(id, &completion(text, position, encoding))
        }
        CodeActionRequest::METHOD => {
            let Some((uri, range)) = code_action_request(&request) else {
                return null_ok(id);
            };
            let text = store.get(&uri).unwrap_or("");
            ok_or_null(id, &code_actions(&uri, text, range, encoding))
        }
        // Any other request — reply null, never hang.
        _ => null_ok(id),
    }
}

/// Decode the `textDocument.uri` of each request kind, logging + dropping a
/// malformed payload. Kept per-kind so each uses its real `Params` type.
fn formatting_uri(request: &Request) -> Option<Uri> {
    decode_uri::<DocumentFormattingParams>(request, |p| p.text_document)
}
fn semantic_tokens_uri(request: &Request) -> Option<Uri> {
    decode_uri::<SemanticTokensParams>(request, |p| p.text_document)
}
fn document_symbol_uri(request: &Request) -> Option<Uri> {
    decode_uri::<DocumentSymbolParams>(request, |p| p.text_document)
}
fn folding_range_uri(request: &Request) -> Option<Uri> {
    decode_uri::<FoldingRangeParams>(request, |p| p.text_document)
}

/// Decode a hover request into `(uri, position)`, dropping a malformed payload.
fn hover_request(request: &Request) -> Option<(Uri, lsp_types::Position)> {
    let params = decode::<HoverParams>(request)?;
    let TextDocumentPositionParams {
        text_document,
        position,
    } = params.text_document_position_params;
    Some((text_document.uri, position))
}

/// Decode a completion request into `(uri, position)`.
fn completion_request(request: &Request) -> Option<(Uri, lsp_types::Position)> {
    let params = decode::<CompletionParams>(request)?;
    let TextDocumentPositionParams {
        text_document,
        position,
    } = params.text_document_position;
    Some((text_document.uri, position))
}

/// Decode a code-action request into `(uri, range)`.
fn code_action_request(request: &Request) -> Option<(Uri, lsp_types::Range)> {
    let params = decode::<CodeActionParams>(request)?;
    Some((params.text_document.uri, params.range))
}

/// Decode a request's params and extract its document identifier's URI.
fn decode_uri<P>(request: &Request, pick: impl FnOnce(P) -> TextDocumentIdentifier) -> Option<Uri>
where
    P: serde::de::DeserializeOwned,
{
    decode::<P>(request).map(|params| pick(params).uri)
}

/// Decode a request's params into `P`, logging + dropping a malformed payload.
fn decode<P>(request: &Request) -> Option<P>
where
    P: serde::de::DeserializeOwned,
{
    match serde_json::from_value::<P>(request.params.clone()) {
        Ok(params) => Some(params),
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
fn publish(connection: &Connection, store: &DocumentStore, encoding: PositionEncoding, uri: &Uri) {
    let text = store.get(uri).unwrap_or("");
    let diags = contain(|| diagnostics(text, encoding))
        .unwrap_or_else(|| vec![internal_error_diagnostic()]);
    publish_diagnostics(connection, uri, diags);
}

/// Send a `textDocument/publishDiagnostics` notification.
fn publish_diagnostics(connection: &Connection, uri: &Uri, diags: Vec<lsp_types::Diagnostic>) {
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
#[path = "main_tests.rs"]
mod transport_tests;
