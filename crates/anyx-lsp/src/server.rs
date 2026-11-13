//! The protocol: messages in and out over standard input and output.

use std::collections::HashMap;
use std::error::Error;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{Completion, FoldingRangeRequest, Formatting, Request as _};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, FoldingRange, FoldingRangeParams,
    FoldingRangeProviderCapability, InsertTextFormat, OneOf, PublishDiagnosticsParams,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
};

use crate::{Language, LineIndex, ProblemKind};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Serves `language` over standard input and output until the client shuts
/// the server down.
pub fn serve(language: &dyn Language) -> Result<()> {
    let (connection, threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![" ".into(), "[".into(), "{".into(), "(".into()]),
            ..CompletionOptions::default()
        }),
        document_formatting_provider: Some(OneOf::Left(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
    };
    connection.initialize(serde_json::to_value(capabilities)?)?;
    let mut server = Server {
        language,
        connection: &connection,
        documents: HashMap::new(),
    };
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                server.request(request)?;
            }
            Message::Notification(notification) => server.notification(notification)?,
            Message::Response(_) => {}
        }
    }
    drop(server);
    drop(connection);
    threads.join()?;
    Ok(())
}

struct Server<'a> {
    language: &'a dyn Language,
    connection: &'a Connection,
    documents: HashMap<Url, String>,
}

impl Server<'_> {
    fn notification(&mut self, notification: Notification) -> Result<()> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                self.documents
                    .insert(uri.clone(), params.text_document.text);
                self.publish(uri)?;
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                // Full sync: the last change is the whole text.
                if let Some(change) = params.content_changes.into_iter().last() {
                    self.documents.insert(uri.clone(), change.text);
                }
                self.publish(uri)?;
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                self.documents.remove(&params.text_document.uri);
            }
            _ => {}
        }
        Ok(())
    }

    fn publish(&self, uri: Url) -> Result<()> {
        let source = &self.documents[&uri];
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
        self.send(Message::Notification(Notification::new(
            PublishDiagnostics::METHOD.into(),
            params,
        )))
    }

    fn request(&self, request: Request) -> Result<()> {
        let id = request.id.clone();
        match request.method.as_str() {
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
                            insert_text: Some(completion.snippet),
                            insert_text_format: Some(InsertTextFormat::SNIPPET),
                            ..CompletionItem::default()
                        })
                        .collect()
                });
                self.respond(id, CompletionResponse::Array(items.unwrap_or_default()))
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
                self.respond(id, edits)
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
                self.respond(id, ranges)
            }
            _ => self.send(Message::Response(Response::new_err(
                id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                format!("found the method `{}`; it is not supported", request.method),
            ))),
        }
    }

    fn with_document<T>(&self, uri: &Url, read: impl FnOnce(&str) -> T) -> Option<T> {
        self.documents.get(uri).map(|source| read(source))
    }

    fn respond(&self, id: RequestId, result: impl serde::Serialize) -> Result<()> {
        self.send(Message::Response(Response::new_ok(id, result)))
    }

    fn send(&self, message: Message) -> Result<()> {
        self.connection.sender.send(message)?;
        Ok(())
    }
}
