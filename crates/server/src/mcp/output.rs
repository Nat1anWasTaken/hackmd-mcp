//! Turning operation results into MCP tool results.

use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::Value;

use super::log;

/// The tool result for a successful call, and what the call log keeps of it.
/// The agent gets pretty JSON, plus the same value as structured content
/// when it is an object.
pub fn success(value: Value) -> (CallToolResult, Value) {
    let text = render(&value);
    let mut result = CallToolResult::success(vec![ContentBlock::text(text.clone())]);
    if value.is_object() {
        result.structured_content = Some(value.clone());
    }
    (result, log::bounded(value, &text))
}

/// How a result reads to the agent.
pub fn render(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// A tool result the agent sees as a failure.
pub fn failure(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn text(result: &CallToolResult) -> &str {
        &result.content[0].as_text().unwrap().text
    }

    #[test]
    fn results_render_for_agents() {
        let (result, logged) = success(json!({ "id": 1 }));
        assert_eq!(result.is_error, Some(false));
        assert_eq!(text(&result), "{\n  \"id\": 1\n}");
        assert_eq!(result.structured_content, Some(json!({ "id": 1 })));
        assert_eq!(logged, json!({ "id": 1 }));

        let (result, _) = success(json!([1, 2]));
        assert_eq!(result.structured_content, None);
        assert_eq!(failure("nope").is_error, Some(true));
    }
}
