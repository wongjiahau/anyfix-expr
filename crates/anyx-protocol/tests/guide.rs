//! The guide an agent reads before its first request.

mod common;

use common::{assert_snapshot, poentaro};

#[test]
fn the_guide_explains_the_protocol_then_shows_the_schema() {
    let guide = anyx_protocol::guide(
        &poentaro(),
        "Send a request as the body of `POST https://example.com/agent`.",
    );
    assert_snapshot("tests/snapshots/guide.md", &guide);
}
