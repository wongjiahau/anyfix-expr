//! The protocol: a [`Server`] answers each message with the messages to send
//! back, so any transport can carry them. [`serve`] carries them over
//! standard input and output; the `wasm` feature, over a Web Worker's.

use std::collections::HashMap;
use std::error::Error;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, FoldingRangeRequest, Formatting, Initialize, Request as _, Shutdown,
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, Documentation, FoldingRange,
    FoldingRangeParams, FoldingRangeProviderCapability, InitializeResult, InsertTextFormat,
    MarkupContent, MarkupKind, OneOf, PublishDiagnosticsParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
};

use crate::{Language, LineIndex, ProblemKind};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Serves `language` over standard input and output until the client tells
/// the server to exit.
pub fn serve(language: &dyn Language) -> Result<()> {
    let (connection, threads) = Connection::stdio();
    let mut server = Server::new(language);
    for message in &connection.receiver {
        for reply in server.handle(message) {
            connection.sender.send(reply)?;
        }
        if server.has_exited() {
            break;
        }
    }
    drop(connection);
    threads.join()?;
    Ok(())
}

/// A language server for one language, apart from how its messages travel.
pub struct Server<'a> {
    language: &'a dyn Language,
    documents: HashMap<Url, String>,
    exited: bool,
}

impl<'a> Server<'a> {
    pub fn new(language: &'a dyn Language) -> Self {
        Server {
            language,
            documents: HashMap::new(),
            exited: false,
        }
    }

    /// Whether the client has sent `exit`, after which the server stops.
    pub fn has_exited(&self) -> bool {
        self.exited
    }

    /// The messages to send back for `message`: a request's response, and
    /// the diagnostics of a document that changed.
    pub fn handle(&mut self, message: Message) -> Vec<Message> {
        match message {
            Message::Request(request) => vec![self.request(request)],
            Message::Notification(notification) if notification.method == Exit::METHOD => {
                self.exited = true;
                Vec::new()
            }
            Message::Notification(notification) => self.notification(notification),
            Message::Response(_) => Vec::new(),
        }
    }

    fn capabilities() -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![" ".into(), "[".into(), "{".into(), "(".into()]),
                ..CompletionOptions::default()
            }),
            document_formatting_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            ..ServerCapabilities::default()
        }
    }
}

impl Server<'_> {
    fn notification(&mut self, notification: Notification) -> Vec<Message> {
        // A notification has no response, so one the server can't read is
        // dropped.
        self.read_notification(notification).unwrap_or_default()
    }

    fn read_notification(&mut self, notification: Notification) -> Result<Vec<Message>> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                self.documents
                    .insert(uri.clone(), params.text_document.text);
                Ok(vec![self.publish(uri)])
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                // Full sync: the last change is the whole text.
                if let Some(change) = params.content_changes.into_iter().last() {
                    self.documents.insert(uri.clone(), change.text);
                }
                Ok(vec![self.publish(uri)])
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                self.documents.remove(&params.text_document.uri);
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }

    fn publish(&self, uri: Url) -> Message {
        let source = self.documents.get(&uri).map_or("", String::as_str);
        let index = LineIndex::new(source);
        let diagnostics = self
            .language
            .check(source)
            .into_iter()
            .map(|problem| Diagnostic {
                range: index.range(problem.span.start, problem.span.end),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some(
                    match problem.kind {
                        ProblemKind::Syntax => "anyx",
                        ProblemKind::Grammar => "grammar",
                    }
                    .into(),
                ),
                message: problem.message,
                ..Diagnostic::default()
            })
            .collect();
        let params = PublishDiagnosticsParams::new(uri, diagnostics, None);
        Message::Notification(Notification::new(PublishDiagnostics::METHOD.into(), params))
    }

    fn request(&mut self, request: Request) -> Message {
        let id = request.id.clone();
        self.read_request(request).unwrap_or_else(|error| {
            Message::Response(Response::new_err(
                id,
                ErrorCode::InvalidParams as i32,
                error.to_string(),
            ))
        })
    }

    fn read_request(&mut self, request: Request) -> Result<Message> {
        let id = request.id.clone();
        Ok(match request.method.as_str() {
            Initialize::METHOD => respond(
                id,
                InitializeResult {
                    capabilities: Self::capabilities(),
                    server_info: None,
                },
            ),
            Shutdown::METHOD => respond(id, ()),
            Completion::METHOD => {
                let params: CompletionParams = serde_json::from_value(request.params)?;
                let position = params.text_document_position;
                let items = self.with_document(&position.text_document.uri, |source| {
                    let offset = LineIndex::new(source).offset(position.position);
                    crate::complete(self.language, source, offset)
                        .into_iter()
                        .map(|completion| CompletionItem {
                            label: completion.label,
                            kind: Some(CompletionItemKind::KEYWORD),
                            detail: Some(completion.detail),
                            documentation: completion.documentation.map(|text| {
                                Documentation::MarkupContent(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: text,
                                })
                            }),
                            insert_text: Some(completion.snippet),
                            insert_text_format: Some(InsertTextFormat::SNIPPET),
                            ..CompletionItem::default()
                        })
                        .collect()
                });
                respond(id, CompletionResponse::Array(items.unwrap_or_default()))
            }
            Formatting::METHOD => {
                let params: DocumentFormattingParams = serde_json::from_value(request.params)?;
                let edits = self
                    .with_document(&params.text_document.uri, |source| {
                        let formatted = self.language.format(source)?;
                        let index = LineIndex::new(source);
                        Some(vec![TextEdit::new(index.range(0, source.len()), formatted)])
                    })
                    .flatten();
                respond(id, edits)
            }
            FoldingRangeRequest::METHOD => {
                let params: FoldingRangeParams = serde_json::from_value(request.params)?;
                let ranges = self.with_document(&params.text_document.uri, |source| {
                    let index = LineIndex::new(source);
                    crate::folds(source)
                        .into_iter()
                        .map(|span| FoldingRange {
                            start_line: index.position(span.start).line,
                            end_line: index.position(span.end).line,
                            ..FoldingRange::default()
                        })
                        .collect::<Vec<_>>()
                });
                respond(id, ranges)
            }
            _ => Message::Response(Response::new_err(
                id,
                ErrorCode::MethodNotFound as i32,
                format!("found the method `{}`; it is not supported", request.method),
            )),
        })
    }

    fn with_document<T>(&self, uri: &Url, read: impl FnOnce(&str) -> T) -> Option<T> {
        self.documents.get(uri).map(|source| read(source))
    }
}

fn respond(id: RequestId, result: impl serde::Serialize) -> Message {
    Message::Response(Response::new_ok(id, result))
}
