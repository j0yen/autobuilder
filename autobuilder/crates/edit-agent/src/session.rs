//! The agentic loop: send a request, receive a response, execute any
//! tool calls, feed results back, repeat until `stop_reason == end_turn`
//! or a budget cuts the session off.
//!
//! The loop is parameterised by [`MessagesApi`] so tests can replay
//! canned responses without hitting the network.

use anyhow::{Context, Result, anyhow};
use serde_json::json;

use crate::api::{ContentBlock, Message, MessagesApi, MessagesRequest, Role, ToolSpec};
use crate::sandbox::Sandbox;
use crate::tools;

/// Inputs to one edit-agent session.
pub struct SessionInput<'a> {
    pub model: &'a str,
    pub max_tokens_per_call: u32,
    /// Hard ceiling on assistant turns; prevents runaway loops.
    pub max_turns: u32,
    /// The system prompt for the edit-agent. Loaded by the caller
    /// (typically from the autobuilder skill prompt) so this crate
    /// stays prompt-agnostic.
    pub system_prompt: &'a str,
    /// Initial user message — usually the rendered `FailureCapsule` plus
    /// any pointers to relevant files.
    pub user_message: &'a str,
    pub sandbox: Sandbox,
}

/// Summary of a completed session.
#[derive(Debug, Default)]
pub struct SessionOutcome {
    pub turns: u32,
    pub tool_calls: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub stop_reason: String,
    /// Complete transcript including initial user message, every assistant
    /// turn, and every tool-result turn. Caller is responsible for logging
    /// this to a turn-log file (deferred to S4 wiring).
    pub transcript: Vec<Message>,
}

/// Drive the edit-agent loop to completion.
///
/// # Errors
/// Returns an error if the API transport fails, the response shape is
/// unexpected, or `max_turns` is exhausted without an `end_turn` stop.
#[allow(clippy::needless_pass_by_value)] // owning `SessionInput` (including the `Sandbox`) reads cleaner at the call site
pub fn run<A: MessagesApi>(api: &A, input: SessionInput<'_>) -> Result<SessionOutcome> {
    let tools = tool_specs();
    let mut transcript: Vec<Message> = Vec::new();
    transcript.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: input.user_message.to_owned(),
        }],
    });

    let mut total_input = 0u32;
    let mut total_output = 0u32;
    let mut tool_calls = 0u32;

    for turn in 0..input.max_turns {
        let request = MessagesRequest {
            model: input.model,
            max_tokens: input.max_tokens_per_call,
            system: Some(input.system_prompt),
            messages: transcript.clone(),
            tools: tools.clone(),
        };
        let response = api
            .send(&request)
            .with_context(|| format!("turn {turn}: messages.send"))?;
        total_input = total_input.saturating_add(response.usage.input_tokens);
        total_output = total_output.saturating_add(response.usage.output_tokens);
        transcript.push(Message {
            role: response.role,
            content: response.content.clone(),
        });

        match response.stop_reason.as_str() {
            "end_turn" | "stop_sequence" => {
                return Ok(SessionOutcome {
                    turns: turn + 1,
                    tool_calls,
                    input_tokens: total_input,
                    output_tokens: total_output,
                    stop_reason: response.stop_reason,
                    transcript,
                });
            }
            "tool_use" => {
                let mut results: Vec<ContentBlock> = Vec::new();
                for block in &response.content {
                    if let ContentBlock::ToolUse { id, name, input: args } = block {
                        tool_calls = tool_calls.saturating_add(1);
                        let output = tools::dispatch(name, args, &input.sandbox)?;
                        results.push(ContentBlock::ToolResult {
                            tool_use_id: id.clone(),
                            content: output.text,
                            is_error: output.is_error,
                        });
                    }
                }
                if results.is_empty() {
                    return Err(anyhow!(
                        "turn {turn}: stop_reason=tool_use but no tool_use blocks in response"
                    ));
                }
                transcript.push(Message {
                    role: Role::User,
                    content: results,
                });
            }
            "max_tokens" => {
                return Err(anyhow!(
                    "turn {turn}: stop_reason=max_tokens (raise max_tokens_per_call)"
                ));
            }
            other => {
                return Err(anyhow!(
                    "turn {turn}: unexpected stop_reason `{other}`"
                ));
            }
        }
    }

    Err(anyhow!(
        "exhausted max_turns ({}) without end_turn",
        input.max_turns
    ))
}

fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "read_file".to_owned(),
            description: "Read a UTF-8 text file inside the sandbox root. Returns the file's contents (truncated if very large) or a soft error.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path, relative to sandbox root or absolute (must be inside the sandbox)."}
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "write_file".to_owned(),
            description: "Create or overwrite a file inside the sandbox root with the given content.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"]
            }),
        },
        ToolSpec {
            name: "edit_file".to_owned(),
            description: "Replace a single, unique occurrence of `old_string` with `new_string` in the file at `path`. Fails if `old_string` is absent or appears more than once.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old_string": {"type": "string"},
                    "new_string": {"type": "string"}
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        ToolSpec {
            name: "bash".to_owned(),
            description: "Run a bash command with cwd = sandbox root. 120-second timeout. Returns exit_code, stdout, stderr.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            }),
        },
    ]
}
