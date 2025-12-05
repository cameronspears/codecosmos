//! LLM-powered suggestions via OpenRouter
//!
//! Tiered approach:
//! - Grok Fast: Quick categorization and summaries (~$0.0001/call)
//! - Opus 4.5: Deep analysis on explicit request (~$0.02/call)

use super::{Priority, Suggestion, SuggestionKind, SuggestionSource};
use crate::config::Config;
use crate::index::FileIndex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Models available for suggestions
#[derive(Debug, Clone, Copy)]
pub enum Model {
    /// Grok Fast - for quick categorization
    GrokFast,
    /// Opus 4.5 - for deep analysis
    Opus,
}

impl Model {
    pub fn id(&self) -> &'static str {
        match self {
            Model::GrokFast => "x-ai/grok-3-fast",
            Model::Opus => "anthropic/claude-sonnet-4",
        }
    }

    pub fn max_tokens(&self) -> u32 {
        match self {
            Model::GrokFast => 1024,
            Model::Opus => 4096,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: String,
}

/// Get API key from config
fn get_api_key() -> Option<String> {
    Config::load().get_api_key()
}

/// Check if LLM is available
pub fn is_available() -> bool {
    get_api_key().is_some()
}

/// Call OpenRouter API
async fn call_llm(system: &str, user: &str, model: Model) -> anyhow::Result<String> {
    let api_key = get_api_key().ok_or_else(|| anyhow::anyhow!("No API key configured"))?;

    let client = reqwest::Client::new();

    let request = ChatRequest {
        model: model.id().to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user.to_string(),
            },
        ],
        max_tokens: model.max_tokens(),
        stream: false,
    };

    let response = client
        .post(OPENROUTER_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .header("HTTP-Referer", "https://github.com/cosmos")
        .header("X-Title", "Cosmos")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, text));
    }

    let chat_response: ChatResponse = response.json().await?;

    chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| anyhow::anyhow!("No response from AI"))
}

/// Quick file summary using Grok Fast
pub async fn quick_summary(path: &PathBuf, content: &str, file_index: &FileIndex) -> anyhow::Result<String> {
    let system = r#"You are a code analyst. Provide a brief summary of what this file does.
Output exactly 1-2 sentences. Be specific and technical."#;

    let user = format!(
        "File: {} ({} lines, {} functions)\n\n{}",
        path.display(),
        file_index.loc,
        file_index.symbols.len(),
        truncate_content(content, 2000)
    );

    call_llm(system, &user, Model::GrokFast).await
}

/// Deep analysis using Opus 4.5 (on-demand only)
pub async fn analyze_file_deep(
    path: &PathBuf,
    content: &str,
    file_index: &FileIndex,
) -> anyhow::Result<Vec<Suggestion>> {
    let system = r#"You are a senior code reviewer. Analyze this file and suggest improvements.

OUTPUT FORMAT (JSON array):
[
  {
    "kind": "improvement|bugfix|feature|optimization|quality|documentation|testing",
    "priority": "high|medium|low",
    "summary": "One-line description",
    "detail": "Explanation with specific recommendations",
    "line": null or line number
  }
]

GUIDELINES:
- Be specific and actionable
- Focus on the most impactful improvements
- Limit to 3-5 suggestions
- Consider: correctness, performance, maintainability, readability
- Only suggest changes that provide real value"#;

    let metrics = format!(
        "Metrics:\n- Lines: {}\n- Functions: {}\n- Complexity: {:.1}\n- Patterns detected: {}",
        file_index.loc,
        file_index.symbols.len(),
        file_index.complexity,
        file_index.patterns.len()
    );

    let user = format!(
        "File: {}\n\n{}\n\nCode:\n```\n{}\n```",
        path.display(),
        metrics,
        truncate_content(content, 8000)
    );

    let response = call_llm(system, &user, Model::Opus).await?;

    parse_suggestions(&response, path)
}

/// Inquiry-based suggestion - user asks "what should I improve?"
pub async fn inquiry(
    path: &PathBuf,
    content: &str,
    file_index: &FileIndex,
    context: Option<&str>,
) -> anyhow::Result<String> {
    let system = r#"You are a thoughtful code companion. The developer is asking for suggestions on what to improve.

Respond conversationally but concisely. Structure your response:

1. **Quick Assessment** (1 sentence)
2. **Top Recommendation** (2-3 sentences)
3. **Why it matters** (1 sentence)

Be specific to this code. Don't be generic."#;

    let context_text = context.map(|c| format!("\nContext: {}", c)).unwrap_or_default();

    let user = format!(
        "File: {} ({} lines)\n\nSymbols: {}\nPatterns found: {}{}\n\nCode:\n```\n{}\n```\n\nWhat should I improve?",
        path.display(),
        file_index.loc,
        file_index.symbols.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", "),
        file_index.patterns.len(),
        context_text,
        truncate_content(content, 4000)
    );

    call_llm(system, &user, Model::GrokFast).await
}

/// Generate a fix/change for a specific suggestion
pub async fn generate_fix(
    path: &PathBuf,
    content: &str,
    suggestion: &Suggestion,
) -> anyhow::Result<String> {
    let system = r#"You are a code improvement assistant. Generate a fix for the described issue.

OUTPUT FORMAT:
1. Brief explanation (2-3 sentences)
2. Code changes in unified diff format:
   --- a/filepath
   +++ b/filepath
   @@ context @@
    unchanged
   -removed
   +added

Be precise. Only change what's necessary."#;

    let user = format!(
        "File: {}\n\nIssue: {}\n{}\n\nCode:\n```\n{}\n```",
        path.display(),
        suggestion.summary,
        suggestion.detail.as_deref().unwrap_or(""),
        truncate_content(content, 6000)
    );

    call_llm(system, &user, Model::Opus).await
}

/// Parse JSON suggestions from LLM response
fn parse_suggestions(response: &str, path: &PathBuf) -> anyhow::Result<Vec<Suggestion>> {
    // Try to extract JSON array from response
    let json_str = if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            &response[start..=end]
        } else {
            response
        }
    } else {
        response
    };

    let parsed: Vec<SuggestionJson> = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse suggestions: {}", e))?;

    let suggestions = parsed
        .into_iter()
        .map(|s| {
            let kind = match s.kind.as_str() {
                "bugfix" => SuggestionKind::BugFix,
                "feature" => SuggestionKind::Feature,
                "optimization" => SuggestionKind::Optimization,
                "quality" => SuggestionKind::Quality,
                "documentation" => SuggestionKind::Documentation,
                "testing" => SuggestionKind::Testing,
                _ => SuggestionKind::Improvement,
            };

            let priority = match s.priority.as_str() {
                "high" => Priority::High,
                "low" => Priority::Low,
                _ => Priority::Medium,
            };

            let mut suggestion = Suggestion::new(
                kind,
                priority,
                path.clone(),
                s.summary,
                SuggestionSource::LlmDeep,
            )
            .with_detail(s.detail);

            if let Some(line) = s.line {
                suggestion = suggestion.with_line(line);
            }

            suggestion
        })
        .collect();

    Ok(suggestions)
}

#[derive(Deserialize)]
struct SuggestionJson {
    kind: String,
    priority: String,
    summary: String,
    detail: String,
    line: Option<usize>,
}

/// Truncate content for API calls
fn truncate_content(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        content.to_string()
    } else {
        // Try to truncate at a line boundary
        let truncated = &content[..max_chars];
        if let Some(last_newline) = truncated.rfind('\n') {
            format!("{}\n... (truncated)", &content[..last_newline])
        } else {
            format!("{}... (truncated)", truncated)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_content() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let truncated = truncate_content(content, 15);
        assert!(truncated.contains("truncated"));
        assert!(truncated.len() < content.len() + 20);
    }

    #[test]
    fn test_parse_suggestions() {
        let json = r#"[
            {
                "kind": "improvement",
                "priority": "high",
                "summary": "Test suggestion",
                "detail": "Test detail",
                "line": 10
            }
        ]"#;

        let path = PathBuf::from("test.rs");
        let suggestions = parse_suggestions(json, &path).unwrap();

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].priority, Priority::High);
    }
}
