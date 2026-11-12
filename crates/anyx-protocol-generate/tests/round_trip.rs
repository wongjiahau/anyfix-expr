//! The generated Rust module, compiled: a client calls a server through a
//! transport that runs `handle` in the same process.

#[path = "generated/poentaro.rs"]
mod poentaro;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use anyx_protocol::client::{CallError, Transport};
use poentaro::{
    Client, CreateContentError, Diagnostic, File, FileSummary, ReadError, Server,
    WriteAtRevisionContentError,
};

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

/// Files by path, with their revision and content, in memory.
#[derive(Default)]
struct Files {
    files: RefCell<BTreeMap<String, (i64, String)>>,
    trashed: Vec<String>,
}

/// The server's own failure.
#[derive(Debug, PartialEq)]
struct Broken;

impl Server for Files {
    type Error = Broken;

    async fn list_files(&self) -> Result<Vec<FileSummary>, Broken> {
        Ok(self
            .files
            .borrow()
            .iter()
            .map(|(path, (revision, _))| FileSummary {
                path: path.clone(),
                revision: *revision,
            })
            .collect())
    }

    async fn read(&self, path: String) -> Result<Result<File, ReadError>, Broken> {
        if path == "broken" {
            return Err(Broken);
        }
        if self.trashed.contains(&path) {
            return Ok(Err(ReadError::Trashed));
        }
        Ok(match self.files.borrow().get(&path) {
            Some((revision, content)) => Ok(File {
                path,
                revision: *revision,
                content: content.clone(),
            }),
            None => Err(ReadError::Missing),
        })
    }

    async fn write_at_revision_content(
        &self,
        path: String,
        revision: i64,
        content: String,
    ) -> Result<Result<i64, WriteAtRevisionContentError>, Broken> {
        let mut files = self.files.borrow_mut();
        let Some((current, saved)) = files.get_mut(&path) else {
            return Ok(Err(WriteAtRevisionContentError::Missing));
        };
        if *current != revision {
            return Ok(Err(
                WriteAtRevisionContentError::ConflictAtRevisionContent {
                    revision: *current,
                    content: saved.clone(),
                },
            ));
        }
        if content.contains('!') {
            return Ok(Err(WriteAtRevisionContentError::Rejected {
                diagnostics: vec![Diagnostic {
                    line: 1,
                    column: 1 + content.find('!').unwrap() as i64,
                    code: "bang".to_string(),
                    message: "found `!`".to_string(),
                }],
            }));
        }
        *current += 1;
        *saved = content;
        Ok(Ok(*current))
    }

    async fn create_content(
        &self,
        path: String,
        content: String,
    ) -> Result<Result<i64, CreateContentError>, Broken> {
        let mut files = self.files.borrow_mut();
        if let Some((revision, _)) = files.get(&path) {
            return Ok(Err(CreateContentError::AlreadyExistsAtRevision {
                revision: *revision,
            }));
        }
        if path.is_empty() {
            return Ok(Err(CreateContentError::InvalidPath {
                message: "found an empty path".to_string(),
            }));
        }
        files.insert(path, (1, content));
        Ok(Ok(1))
    }
}

/// Sends each request straight to `handle`, keeping what it logged.
struct InProcess {
    server: Files,
    logged: RefCell<Vec<Broken>>,
}

impl Transport for InProcess {
    type Error = std::convert::Infallible;

    async fn send(&self, request: String) -> Result<String, Self::Error> {
        Ok(poentaro::handle(&self.server, &request, |error| {
            self.logged.borrow_mut().push(error)
        })
        .await)
    }
}

fn client() -> Client<InProcess> {
    Client::new(InProcess {
        server: Files {
            trashed: vec!["old.poentaro".to_string()],
            ..Files::default()
        },
        logged: RefCell::new(Vec::new()),
    })
}

#[test]
fn a_client_creates_reads_and_writes_a_file() {
    let client = client();
    assert_eq!(
        run(client.create_content("a.poentaro".into(), "1 2 3".into())),
        Ok(1)
    );
    assert_eq!(
        run(client.read("a.poentaro".into())),
        Ok(File {
            path: "a.poentaro".into(),
            revision: 1,
            content: "1 2 3".into()
        })
    );
    assert_eq!(
        run(client.write_at_revision_content("a.poentaro".into(), 1, "line\n\"quoted\"".into())),
        Ok(2)
    );
    assert_eq!(
        run(client.list_files()),
        Ok(vec![FileSummary {
            path: "a.poentaro".into(),
            revision: 2
        }])
    );
}

#[test]
fn a_method_error_reaches_the_client_with_its_arguments() {
    let client = client();
    run(client.create_content("a.poentaro".into(), "1".into())).unwrap();
    assert_eq!(
        run(client.write_at_revision_content("a.poentaro".into(), 7, "2".into())),
        Err(CallError::Method(
            WriteAtRevisionContentError::ConflictAtRevisionContent {
                revision: 1,
                content: "1".into()
            }
        ))
    );
    assert_eq!(
        run(client.write_at_revision_content("a.poentaro".into(), 1, "2 !".into())),
        Err(CallError::Method(WriteAtRevisionContentError::Rejected {
            diagnostics: vec![Diagnostic {
                line: 1,
                column: 3,
                code: "bang".into(),
                message: "found `!`".into()
            }]
        }))
    );
    assert_eq!(
        run(client.read("old.poentaro".into())),
        Err(CallError::Method(ReadError::Trashed))
    );
    assert_eq!(
        run(client.create_content("".into(), "1".into())),
        Err(CallError::Method(CreateContentError::InvalidPath {
            message: "found an empty path".into()
        }))
    );
}

#[test]
fn an_internal_error_is_logged_and_stops_the_calls_after_it() {
    let client = client();
    let response = run(poentaro::handle(
        &client.transport.server,
        r#"create "a" content "1", read "broken", read "a""#,
        |error| client.transport.logged.borrow_mut().push(error),
    ));
    assert_eq!(response, "1,\nINTERNAL ERROR,\nNOT RUN,\n");
    assert_eq!(*client.transport.logged.borrow(), vec![Broken]);
    assert_eq!(run(client.read("broken".into())), Err(CallError::Internal));
}

#[test]
fn every_call_runs_even_after_a_method_error() {
    let client = client();
    let response = run(poentaro::handle(
        &client.transport.server,
        r#"read "a", create "a" content "1", read "a""#,
        |_| unreachable!(),
    ));
    assert_eq!(
        response,
        "ERROR missing,\n1,\n{ path \"a\", revision 1, content \"1\" },\n"
    );
}

#[test]
fn a_request_that_cannot_be_read_gets_invalid_request() {
    let client = client();
    let response = run(poentaro::handle(
        &client.transport.server,
        "delete \"a\"",
        |_| unreachable!(),
    ));
    assert!(response.starts_with("INVALID REQUEST"), "{response}");
}
