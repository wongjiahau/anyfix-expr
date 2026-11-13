//! The Rust module generated from a schema of scopes, compiled: a server
//! implements a `Server` trait per scope, and a client reaches each scope's
//! methods through `client.files().export()`.

#[path = "generated/scoped.rs"]
mod notes;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use anyx_protocol::client::{CallError, Transport};
use notes::common::Diagnostic;
use notes::files::export::{HtmlError, Page, PdfError};
use notes::files::{File, ReadError, WriteAtRevisionContentError};
use notes::{Client, files};

/// Runs a future that never waits, as every future here is.
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the future waited"),
    }
}

/// Notes by path, with their revision and content.
struct Notes {
    notes: RefCell<BTreeMap<String, (i64, String)>>,
}

impl notes::Server for Notes {
    type Error = std::convert::Infallible;
}

impl files::Server for Notes {
    async fn list(&self) -> Result<Vec<String>, Self::Error> {
        Ok(self.notes.borrow().keys().cloned().collect())
    }

    async fn read(&self, path: String) -> Result<Result<File, ReadError>, Self::Error> {
        Ok(match path.as_str() {
            "old" => Err(ReadError::Trashed),
            _ => match self.notes.borrow().get(&path) {
                Some((revision, content)) => Ok(File {
                    path,
                    revision: *revision,
                    content: content.clone(),
                }),
                None => Err(ReadError::Missing),
            },
        })
    }

    async fn write_at_revision_content(
        &self,
        path: String,
        revision: i64,
        content: String,
    ) -> Result<Result<Vec<Diagnostic>, WriteAtRevisionContentError>, Self::Error> {
        if content.is_empty() {
            return Ok(Err(WriteAtRevisionContentError::Rejected {
                diagnostics: vec![Diagnostic {
                    line: 1,
                    message: "empty".into(),
                }],
            }));
        }
        self.notes
            .borrow_mut()
            .insert(path, (revision + 1, content));
        Ok(Ok(vec![]))
    }
}

impl files::export::Server for Notes {
    async fn html(&self, path: String) -> Result<Result<Vec<Page>, HtmlError>, Self::Error> {
        Ok(match self.notes.borrow().get(&path) {
            Some((_, content)) => Ok(vec![Page {
                html: format!("<p>{content}</p>"),
                problem: None,
            }]),
            None => Err(HtmlError::Missing),
        })
    }

    async fn pdf(&self, path: String) -> Result<Result<Vec<u8>, PdfError>, Self::Error> {
        Ok(match self.notes.borrow().get(&path) {
            Some((_, content)) => Ok(content.as_bytes().to_vec()),
            None => Err(PdfError::Unreadable {
                diagnostics: vec![],
            }),
        })
    }
}

struct InProcess {
    server: Notes,
}

impl Transport for InProcess {
    type Error = std::convert::Infallible;

    async fn send(&self, request: String) -> Result<String, Self::Error> {
        Ok(notes::handle(&self.server, &request, |error| match error {}).await)
    }
}

fn client() -> Client<InProcess> {
    Client::new(InProcess {
        server: Notes {
            notes: RefCell::new(BTreeMap::new()),
        },
    })
}

#[test]
fn a_client_calls_the_methods_of_each_scope() {
    let client = client();
    assert_eq!(
        run(client
            .files()
            .write_at_revision_content("a".into(), 0, "hi".into())),
        Ok(vec![])
    );
    assert_eq!(run(client.files().list()), Ok(vec!["a".to_string()]));
    assert_eq!(
        run(client.files().read("a".into())),
        Ok(File {
            path: "a".into(),
            revision: 1,
            content: "hi".into()
        })
    );
    assert_eq!(
        run(client.files().export().html("a".into())),
        Ok(vec![Page {
            html: "<p>hi</p>".into(),
            problem: None
        }])
    );
    assert_eq!(
        run(client.files().export().pdf("a".into())),
        Ok(b"hi".to_vec())
    );
}

#[test]
fn errors_from_other_scopes_reach_the_client() {
    let client = client();
    assert_eq!(
        run(client.files().read("old".into())),
        Err(CallError::Method(ReadError::Trashed))
    );
    assert_eq!(
        run(client.files().export().html("b".into())),
        Err(CallError::Method(HtmlError::Missing))
    );
    assert_eq!(
        run(client
            .files()
            .write_at_revision_content("a".into(), 0, String::new())),
        Err(CallError::Method(WriteAtRevisionContentError::Rejected {
            diagnostics: vec![Diagnostic {
                line: 1,
                message: "empty".into()
            }]
        }))
    );
}

#[test]
fn a_request_calls_each_method_by_its_full_name() {
    let server = Notes {
        notes: RefCell::new(BTreeMap::from([("a".to_string(), (1, "hi".to_string()))])),
    };
    let response = run(notes::handle(
        &server,
        "files/list, files/export/pdf \"a\"",
        |error| match error {},
    ));
    assert_eq!(response, "[\"a\"],\n\"aGk=\",\n");
}

#[test]
fn the_client_reads_the_guide_through_schema() {
    let guide = run(client().schema()).unwrap();
    assert_eq!(guide, anyx_protocol::guide(notes::schema(), ""));
    assert!(guide.contains("files/export/pdf"), "{guide}");
}
