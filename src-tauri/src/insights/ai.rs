//! The Anthropic Messages API client, over raw HTTP.
//!
//! There is no official Anthropic SDK for Rust, so this speaks the REST API directly. Four
//! things it is built to get right, each of them a way this call fails in practice:
//!
//! - **The model must be able to read the live web.** A crypto answer from training data alone
//!   is worse than useless, so every request declares the server-side `web_search` tool. Its
//!   type string is versioned *and* model-gated — the current variant is rejected on older
//!   models — which is what [`web_search_tool_type`] exists to decide.
//! - **A server-tool turn can pause.** When the model runs a long search chain the API answers
//!   `stop_reason: "pause_turn"` with a partial turn. That is not an error and not an answer:
//!   the assistant content has to be echoed back to continue. Treating it as a reply is how
//!   this feature would silently return half a report.
//! - **A server-tool failure arrives as HTTP 200.** A search error is a `web_search_tool_result`
//!   block whose `content` is an *object* rather than the usual *list*. Indexing it blindly is
//!   a panic; ignoring the distinction reports "no sources found" for a hard failure.
//! - **Prose leaks into JSON.** The model is asked for a bare JSON object and usually obeys,
//!   but a stray sentence or a ```json fence around it must not fail a call that has already
//!   been billed — hence [`extract_json_object`].

use serde::Deserialize;
use std::time::Duration;

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

/// Long, because a single call runs a chain of web searches on Anthropic's side before the
/// first token comes back. The panel shows a spinner for the whole of it.
const TIMEOUT: Duration = Duration::from_secs(180);

/// How many times a `pause_turn` may be resumed before this gives up. Three covers the deep
/// search chains this feature asks for; an unbounded loop would be an unbounded bill.
const MAX_CONTINUATIONS: u8 = 3;

/// Models that accept the dynamic-filtering web search tool. Everything else falls back to the
/// basic variant — sending the new type to an older model is a 400, not a graceful downgrade.
const DYNAMIC_SEARCH_MODELS: [&str; 6] = [
    "claude-opus-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-5",
    "claude-sonnet-4-6",
];

pub fn web_search_tool_type(model: &str) -> &'static str {
    if DYNAMIC_SEARCH_MODELS.contains(&model) {
        "web_search_20260209"
    } else {
        "web_search_20250305"
    }
}

/// One completed call: the text the model wrote, the pages it actually opened, and the bill.
#[derive(Debug, Clone, Default)]
pub struct AiReply {
    pub text: String,
    pub sources: Vec<super::Source>,
    pub usage: super::Usage,
}

pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
    max_searches: u32,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String, max_searches: u32) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .unwrap_or_default(),
            api_key,
            model,
            max_searches,
        }
    }

    /// Runs one prompt to completion, resuming through any `pause_turn` the search chain
    /// produces. `system` is sent as the top-level system prompt; `user` is the only message.
    pub async fn ask(&self, system: &str, user: &str) -> Result<AiReply, String> {
        // The conversation grows by one assistant turn per continuation. `messages` is rebuilt
        // rather than appended to in place because a resumed turn has to carry every previous
        // assistant block — including the search results — or the model starts over.
        let mut messages = vec![serde_json::json!({ "role": "user", "content": user })];
        let mut reply = AiReply::default();

        for _ in 0..=MAX_CONTINUATIONS {
            let body = serde_json::json!({
                "model": self.model,
                // The report is a compact JSON object; this is headroom, not a target.
                "max_tokens": 8000,
                "system": system,
                "messages": messages,
                "tools": [{
                    "type": web_search_tool_type(&self.model),
                    "name": "web_search",
                    "max_uses": self.max_searches,
                }],
            });

            let response = self
                .http
                .post(ENDPOINT)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| describe_transport_error(&e))?;

            let status = response.status();
            let raw = response
                .text()
                .await
                .map_err(|e| format!("the API answered with an unreadable body: {e}"))?;

            if !status.is_success() {
                return Err(describe_api_error(status.as_u16(), &raw));
            }

            let parsed: ApiResponse = serde_json::from_str(&raw)
                .map_err(|e| format!("could not read the API response: {e}"))?;

            reply.usage.input_tokens += parsed.usage.input_tokens;
            reply.usage.output_tokens += parsed.usage.output_tokens;
            reply.usage.web_searches += parsed
                .usage
                .server_tool_use
                .map(|u| u.web_search_requests)
                .unwrap_or(0);

            // A block shape this build does not know is normal traffic, not a failure: the
            // untyped array above is still echoed back intact if the turn pauses.
            let blocks: Vec<ContentBlock> =
                serde_json::from_value(parsed.content.clone()).unwrap_or_default();

            for block in &blocks {
                match block {
                    ContentBlock::Text { text } => reply.text.push_str(text),
                    ContentBlock::WebSearchToolResult { content } => {
                        // An error arrives here as an object, a success as a list — see the
                        // module docs. `SearchResults` models exactly that fork.
                        if let SearchResults::Results(results) = content {
                            for result in results {
                                let source = super::Source {
                                    title: if result.title.trim().is_empty() {
                                        result.url.clone()
                                    } else {
                                        result.title.clone()
                                    },
                                    url: result.url.clone(),
                                };
                                if !reply.sources.iter().any(|s| s.url == source.url) {
                                    reply.sources.push(source);
                                }
                            }
                        }
                    }
                    ContentBlock::Other => {}
                }
            }

            match parsed.stop_reason.as_deref() {
                // Not an answer: the turn was suspended mid-search and has to be handed back
                // verbatim to continue.
                Some("pause_turn") => {
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": parsed.content,
                    }));
                }
                Some("refusal") => {
                    return Err("the model declined to answer this request".into());
                }
                _ => return Ok(reply),
            }
        }

        // Every continuation spent and still paused. Whatever text arrived is returned rather
        // than discarded — the caller decides whether it parses into a report.
        Ok(reply)
    }
}

/// Pulls the outermost JSON object out of a model reply that may be wrapped in a ```json fence
/// or trailed by a sentence. Returns the slice, not a parsed value, so the caller keeps its own
/// `serde` target type.
pub fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, &byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            // A brace inside a string literal — a URL fragment, a headline — must not move the
            // depth counter, or the object ends in the wrong place.
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=index]);
                }
            }
            _ => {}
        }
    }
    None
}

fn describe_transport_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "the request timed out — a deep search can take a couple of minutes, try again or lower the search budget".into()
    } else if error.is_connect() {
        "could not reach api.anthropic.com — check the network connection".into()
    } else {
        format!("request failed: {error}")
    }
}

/// Turns an API error body into something actionable. The status alone is not: 400 covers both
/// "this model does not exist" and "the request was malformed", and only the message separates
/// them.
fn describe_api_error(status: u16, body: &str) -> String {
    let message = serde_json::from_str::<ApiError>(body)
        .ok()
        .map(|e| e.error.message)
        .unwrap_or_else(|| body.chars().take(300).collect());

    match status {
        401 => format!("the API key was rejected: {message}"),
        403 => format!("this API key is not allowed to use that model: {message}"),
        429 => format!("rate limited by the API — wait a moment and retry: {message}"),
        529 => format!("the API is overloaded — retry shortly: {message}"),
        500..=599 => format!("the API failed ({status}): {message}"),
        _ => format!("the API rejected the request ({status}): {message}"),
    }
}

#[derive(Debug, Deserialize)]
struct ApiError {
    error: ApiErrorBody,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    /// Kept untyped, and typed blocks are read *out* of it in `ask`, because the same array
    /// serves two purposes: this file reads text and search results from it, and a `pause_turn`
    /// has to echo it back **verbatim** to resume. Re-encoding it from typed blocks would drop
    /// the fields this file does not model, and the API rejects a resumed turn whose tool
    /// blocks came back incomplete.
    #[serde(default)]
    content: serde_json::Value,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: ApiUsage,
}

#[derive(Debug, Default, Deserialize)]
struct ApiUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    server_tool_use: Option<ServerToolUse>,
}

#[derive(Debug, Deserialize)]
struct ServerToolUse {
    #[serde(default)]
    web_search_requests: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult { content: SearchResults },
    /// `thinking`, `server_tool_use`, and anything the API adds later. Unknown block types are
    /// normal traffic, not a parse failure.
    #[serde(other)]
    Other,
}

/// A search result block is a list on success and an error object on failure. Both are HTTP
/// 200; the untagged enum is what keeps the failure from being read as an empty result set.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SearchResults {
    Results(Vec<SearchResult>),
    /// Carries no payload on purpose: nothing downstream acts on *which* search failed, and a
    /// field no one reads is a field that drifts. The fork itself is the whole point — a failed
    /// search must not deserialize as an empty result list.
    Error {},
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    #[serde(default)]
    title: String,
    url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_search_tool_goes_to_models_that_accept_it() {
        assert_eq!(web_search_tool_type("claude-opus-5"), "web_search_20260209");
        assert_eq!(web_search_tool_type("claude-sonnet-5"), "web_search_20260209");
    }

    #[test]
    fn an_older_model_gets_the_basic_search_tool() {
        // Sending the dynamic-filtering variant to Haiku 4.5 is a 400 — the whole call fails
        // before a single search runs.
        assert_eq!(web_search_tool_type("claude-haiku-4-5"), "web_search_20250305");
    }

    #[test]
    fn json_survives_a_markdown_fence() {
        let reply = "Here you go:\n```json\n{\"verdict\":\"bullish\"}\n```\nHope that helps.";
        assert_eq!(extract_json_object(reply), Some("{\"verdict\":\"bullish\"}"));
    }

    #[test]
    fn a_brace_inside_a_string_does_not_end_the_object() {
        // A headline or a URL containing a brace used to truncate the object here, which
        // presented as "the model returned invalid JSON" on a perfectly good reply.
        let reply = r#"{"title":"Set {the} record","score":9}"#;
        assert_eq!(extract_json_object(reply), Some(reply));
    }

    #[test]
    fn a_nested_object_is_returned_whole() {
        let reply = r#"prefix {"a":{"b":[1,2]},"c":"}"} suffix"#;
        assert_eq!(extract_json_object(reply), Some(r#"{"a":{"b":[1,2]},"c":"}"}"#));
    }

    #[test]
    fn a_reply_with_no_object_is_none() {
        assert_eq!(extract_json_object("I could not find anything."), None);
        assert_eq!(extract_json_object("{ unterminated"), None);
    }

    #[test]
    fn a_search_failure_is_not_read_as_an_empty_result_list() {
        // HTTP 200, but the tool failed. Parsing this as `Vec<SearchResult>` is the bug: it
        // would report a successful call with no sources.
        let block: SearchResults =
            serde_json::from_str(r#"{"type":"web_search_tool_result_error","error_code":"max_uses_exceeded"}"#)
                .unwrap();
        assert!(matches!(block, SearchResults::Error {}));
    }

    #[test]
    fn unknown_content_blocks_do_not_fail_the_parse() {
        let response: ApiResponse = serde_json::from_str(
            r#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"hi"}],
                "stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":2}}"#,
        )
        .unwrap();
        let blocks: Vec<ContentBlock> = serde_json::from_value(response.content.clone()).unwrap();
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0], ContentBlock::Other));
        assert!(matches!(blocks[1], ContentBlock::Text { .. }));
        assert_eq!(response.usage.input_tokens, 10);
    }

    #[test]
    fn a_401_body_becomes_an_actionable_message() {
        let message = describe_api_error(
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
        );
        assert!(message.contains("key was rejected"), "{message}");
        assert!(message.contains("invalid x-api-key"), "{message}");
    }

    #[test]
    fn a_non_json_error_body_is_still_reported() {
        let message = describe_api_error(502, "<html>bad gateway</html>");
        assert!(message.contains("502"), "{message}");
    }
}
