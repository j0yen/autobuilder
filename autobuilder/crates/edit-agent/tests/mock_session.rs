//! Integration test: drive `session::run` against a canned `MessagesApi`
//! implementation so the agentic loop exercises tool dispatch end-to-end
//! without hitting the network.
//!
//! Scenario: the mock client responds in three turns —
//! 1. `stop_reason=tool_use` with one `write_file` call,
//! 2. `stop_reason=tool_use` with one `bash` call,
//! 3. `stop_reason=end_turn` with a goodbye message.
//! Then we assert: turns=3, tool_calls=2, the file was created, and the
//! transcript shape matches the expected user→assistant→user→…→end-turn
//! pattern.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::doc_lazy_continuation,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::cell::RefCell;

use autobuilder_edit_agent::api::{
    ContentBlock, MessagesApi, MessagesRequest, MessagesResponse, Role, Usage,
};
use autobuilder_edit_agent::{Sandbox, SessionInput, run};
use tempfile::TempDir;

struct MockClient {
    responses: RefCell<Vec<MessagesResponse>>,
    requests_seen: RefCell<u32>,
}

impl MockClient {
    fn new(responses: Vec<MessagesResponse>) -> Self {
        Self {
            responses: RefCell::new(responses),
            requests_seen: RefCell::new(0),
        }
    }
}

impl MessagesApi for MockClient {
    fn send(&self, _request: &MessagesRequest<'_>) -> anyhow::Result<MessagesResponse> {
        *self.requests_seen.borrow_mut() += 1;
        let mut q = self.responses.borrow_mut();
        if q.is_empty() {
            anyhow::bail!("mock: no more queued responses");
        }
        Ok(q.remove(0))
    }
}

fn tool_use(id: &str, name: &str, input: serde_json::Value) -> MessagesResponse {
    MessagesResponse {
        id: format!("msg_{id}"),
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: id.to_owned(),
            name: name.to_owned(),
            input,
        }],
        model: "mock".to_owned(),
        stop_reason: "tool_use".to_owned(),
        usage: Usage {
            input_tokens: 10,
            output_tokens: 20,
        },
    }
}

fn end_turn(text: &str) -> MessagesResponse {
    MessagesResponse {
        id: "msg_end".to_owned(),
        role: Role::Assistant,
        content: vec![ContentBlock::Text {
            text: text.to_owned(),
        }],
        model: "mock".to_owned(),
        stop_reason: "end_turn".to_owned(),
        usage: Usage {
            input_tokens: 5,
            output_tokens: 7,
        },
    }
}

#[test]
fn session_runs_tool_loop_to_end_turn() {
    let tmp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(tmp.path()).unwrap();

    let mock = MockClient::new(vec![
        tool_use(
            "tu_1",
            "write_file",
            serde_json::json!({"path": "hello.txt", "content": "hi"}),
        ),
        tool_use(
            "tu_2",
            "bash",
            serde_json::json!({"command": "ls hello.txt"}),
        ),
        end_turn("done"),
    ]);

    let outcome = run(
        &mock,
        SessionInput {
            model: "mock-model",
            max_tokens_per_call: 1024,
            max_turns: 10,
            system_prompt: "test system",
            user_message: "make a file then list it",
            sandbox,
        },
    )
    .unwrap();

    assert_eq!(outcome.turns, 3);
    assert_eq!(outcome.tool_calls, 2);
    assert_eq!(outcome.stop_reason, "end_turn");
    assert_eq!(outcome.input_tokens, 25);
    assert_eq!(outcome.output_tokens, 47);
    assert_eq!(*mock.requests_seen.borrow(), 3);

    let written = std::fs::read_to_string(tmp.path().join("hello.txt")).unwrap();
    assert_eq!(written, "hi");

    let roles: Vec<_> = outcome.transcript.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::User,      // initial prompt
            Role::Assistant, // tool_use #1
            Role::User,      // tool_result #1
            Role::Assistant, // tool_use #2
            Role::User,      // tool_result #2
            Role::Assistant, // end_turn
        ]
    );
}

#[test]
fn session_aborts_on_max_turns_exhausted() {
    let tmp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(tmp.path()).unwrap();

    let mock = MockClient::new(vec![
        tool_use("a", "bash", serde_json::json!({"command": "true"})),
        tool_use("b", "bash", serde_json::json!({"command": "true"})),
        tool_use("c", "bash", serde_json::json!({"command": "true"})),
    ]);

    let err = run(
        &mock,
        SessionInput {
            model: "mock-model",
            max_tokens_per_call: 1024,
            max_turns: 2,
            system_prompt: "sys",
            user_message: "loop forever",
            sandbox,
        },
    )
    .unwrap_err();
    assert!(
        format!("{err}").contains("exhausted max_turns"),
        "expected max_turns error, got: {err}"
    );
}

#[test]
fn session_surfaces_tool_errors_back_to_model() {
    let tmp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(tmp.path()).unwrap();

    // Turn 1: model tries to read a nonexistent file — tool returns a soft
    // error. Turn 2: model gives up gracefully. Verifies the loop wires
    // tool errors back through the transcript instead of erroring out.
    let mock = MockClient::new(vec![
        tool_use(
            "tu_err",
            "read_file",
            serde_json::json!({"path": "does-not-exist.txt"}),
        ),
        end_turn("file missing, giving up"),
    ]);

    let outcome = run(
        &mock,
        SessionInput {
            model: "mock-model",
            max_tokens_per_call: 1024,
            max_turns: 5,
            system_prompt: "sys",
            user_message: "read a missing file",
            sandbox,
        },
    )
    .unwrap();

    assert_eq!(outcome.turns, 2);
    assert_eq!(outcome.tool_calls, 1);
    let tool_result_msg = &outcome.transcript[2];
    let ContentBlock::ToolResult { is_error, .. } = &tool_result_msg.content[0] else {
        panic!("expected ToolResult in transcript[2]");
    };
    assert!(*is_error, "tool result must carry is_error=true");
}
