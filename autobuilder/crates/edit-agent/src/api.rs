//! Minimal Anthropic Messages API client and types.
//!
//! Models only the surface this crate actually uses: `model`, `max_tokens`,
//! `system`, `messages`, and `tools` in requests; `id`, `role`, `content`,
//! `stop_reason`, and `usage` in responses. [`ContentBlock`] is the variant
//! type carrying `text`, `tool_use`, and `tool_result` blocks. Streaming,
//! image inputs, prompt caching, and citation features are deliberately
//! not modeled — when they're needed, add new variants alongside the
//! existing three.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Request body for `POST /v1/messages`.
#[derive(Debug, Serialize)]
pub struct MessagesRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<&'a str>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
}

/// One message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Discriminated content block (text / tool-use / tool-result).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default, skip_serializing_if = "is_false")]
        is_error: bool,
    },
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde signature
fn is_false(b: &bool) -> bool {
    !*b
}

/// Tool definition advertised to the model.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Response body shape for `POST /v1/messages`.
#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    #[allow(dead_code)] // surfaced in logs; not consumed yet
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    #[allow(dead_code)]
    pub model: String,
    pub stop_reason: String,
    pub usage: Usage,
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Abstraction over "call the Messages API." A real implementation hits
/// the network; mock implementations replay canned responses for tests.
pub trait MessagesApi {
    /// Send a request and return the response. Errors are bubbled as
    /// `anyhow::Error`; the trait deliberately does not encode retryable
    /// vs terminal failures — the agentic loop above this owns retry.
    ///
    /// # Errors
    /// Returns an error on transport failure (network, TLS, timeout) or
    /// on an unparseable API response body.
    fn send(&self, request: &MessagesRequest<'_>) -> Result<MessagesResponse>;
}

/// Real Anthropic Messages API client backed by `ureq`.
pub struct AnthropicClient {
    api_key: String,
    base_url: String,
    agent: ureq::Agent,
}

impl AnthropicClient {
    /// Construct a client with the given key. Reads the URL from
    /// `ANTHROPIC_BASE_URL` if set, otherwise the public endpoint.
    #[must_use]
    pub fn new(api_key: String) -> Self {
        let base_url =
            std::env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Self {
            api_key,
            base_url,
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(120))
                .build(),
        }
    }
}

impl MessagesApi for AnthropicClient {
    fn send(&self, request: &MessagesRequest<'_>) -> Result<MessagesResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let body =
            serde_json::to_string(request).context("serializing MessagesRequest")?;
        let resp = self
            .agent
            .post(&url)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .send_string(&body);

        match resp {
            Ok(r) => {
                let text = r.into_string().context("reading response body")?;
                serde_json::from_str(&text)
                    .with_context(|| format!("parsing Messages API response: {text}"))
            }
            Err(ureq::Error::Status(code, r)) => {
                let body_text = r.into_string().unwrap_or_default();
                Err(anyhow!(
                    "Messages API returned HTTP {code}: {body_text}"
                ))
            }
            Err(e) => Err(anyhow!("Messages API transport error: {e}")),
        }
    }
}
