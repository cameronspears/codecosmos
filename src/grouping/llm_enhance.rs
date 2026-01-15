//! LLM-based enhancement for file grouping
//!
//! Optionally refines layer and feature assignments for ambiguous files
//! using LLM analysis of file purpose, exports, and dependencies.

use super::{CodebaseGrouping, Confidence, Feature, Layer};
use crate::index::CodebaseIndex;
use crate::suggest::llm::{is_available, Model, Usage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum files to send to LLM in a single batch
const MAX_BATCH_SIZE: usize = 50;

/// Minimum misc bucket size to trigger LLM enhancement
const MIN_MISC_FOR_LLM: usize = 5;

/// File context for LLM analysis (compact representation)
#[derive(Debug, Serialize)]
struct FileContext {
    path: String,
    purpose: String,
    exports: Vec<String>,
    depends_on: Vec<String>,
    used_by: Vec<String>,
    current_layer: String,
}

/// LLM response for a file's grouping
#[derive(Debug, Deserialize)]
struct FileGroupingResponse {
    path: String,
    layer: String,
    feature: Option<String>,
    confidence: String,
}

/// Batch response from LLM
#[derive(Debug, Deserialize)]
struct GroupingResponse {
    files: Vec<FileGroupingResponse>,
}

/// Check if LLM enhancement should be attempted
pub fn should_enhance(grouping: &CodebaseGrouping) -> bool {
    if !is_available() {
        return false;
    }
    
    // Check if there are enough ambiguous files to warrant LLM
    let low_confidence_count = grouping.low_confidence_files().len();
    let misc_count = count_misc_files(grouping);
    
    low_confidence_count >= MIN_MISC_FOR_LLM || misc_count >= MIN_MISC_FOR_LLM
}

/// Count files in "other" feature groups
fn count_misc_files(grouping: &CodebaseGrouping) -> usize {
    grouping.groups.values()
        .flat_map(|g| g.features.iter())
        .filter(|f| f.name.starts_with("other"))
        .map(|f| f.files.len())
        .sum()
}

/// Collect files that need LLM enhancement
fn collect_ambiguous_files(grouping: &CodebaseGrouping) -> Vec<PathBuf> {
    let mut files = Vec::new();
    
    // Low confidence layer assignments
    for path in grouping.low_confidence_files() {
        files.push(path.clone());
    }
    
    // Files in misc/other features
    for group in grouping.groups.values() {
        for feature in &group.features {
            if feature.name.starts_with("other") {
                files.extend(feature.files.clone());
            }
        }
    }
    
    // Deduplicate
    files.sort();
    files.dedup();
    files
}

/// Build compact file context for LLM
fn build_file_context(path: &PathBuf, index: &CodebaseIndex, grouping: &CodebaseGrouping) -> Option<FileContext> {
    let file_index = index.files.get(path)?;
    let assignment = grouping.file_assignments.get(path)?;
    
    Some(FileContext {
        path: path.to_string_lossy().to_string(),
        purpose: file_index.summary.purpose.clone(),
        exports: file_index.summary.exports.iter().take(5).cloned().collect(),
        depends_on: file_index.summary.depends_on.iter()
            .take(5)
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        used_by: file_index.summary.used_by.iter()
            .take(5)
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        current_layer: format!("{:?}", assignment.layer),
    })
}

/// Build the system prompt for grouping enhancement
fn build_system_prompt() -> &'static str {
    r#"You are a code organization expert. Your job is to categorize files into the correct architectural layer and suggest meaningful feature groupings.

LAYERS (choose one):
- Frontend: UI components, pages, layouts, styles, hooks, context providers
- Backend: Server logic, services, controllers, middleware, workers, jobs
- API: Route handlers, endpoints, API definitions, REST/GraphQL resolvers
- Database: Models, entities, schemas, migrations, repositories, queries
- Shared: Types, utilities, helpers, constants, interfaces
- Config: Configuration files
- Tests: Test files
- Infra: CI/CD, Docker, deployment, scripts
- Unknown: Only if truly unclassifiable

OUTPUT FORMAT (JSON):
{
  "files": [
    {
      "path": "path/to/file.ts",
      "layer": "Backend",
      "feature": "authentication",
      "confidence": "high"
    }
  ]
}

FEATURE NAMING:
- Use lowercase, single-word or hyphenated names
- Group related files together (e.g., "auth", "user-profile", "checkout")
- Use null for feature if file doesn't fit any logical grouping

GUIDELINES:
- Consider the file's PURPOSE, not just its location
- Look at what it EXPORTS and what DEPENDS on it
- Files used by UI components → Frontend
- Files that serve data/handle requests → Backend or API
- Route handlers and endpoint definitions → API
- Data models and database queries → Database
- Shared utilities and types → Shared"#
}

/// Build the user prompt with file contexts
fn build_user_prompt(files: &[FileContext]) -> String {
    let mut prompt = String::from("Analyze these files and provide the correct layer and feature grouping:\n\n");
    
    for file in files {
        prompt.push_str(&format!(
            "File: {}\n  Purpose: {}\n  Exports: {}\n  Depends on: {}\n  Used by: {}\n  Current: {}\n\n",
            file.path,
            file.purpose,
            file.exports.join(", "),
            file.depends_on.join(", "),
            file.used_by.join(", "),
            file.current_layer,
        ));
    }
    
    prompt.push_str("\nProvide JSON response with corrected layer and feature for each file.");
    prompt
}

/// Parse layer from string
fn parse_layer(s: &str) -> Layer {
    match s.to_lowercase().as_str() {
        "frontend" => Layer::Frontend,
        "backend" => Layer::Backend,
        "api" => Layer::API,
        "database" => Layer::Database,
        "shared" => Layer::Shared,
        "config" => Layer::Config,
        "tests" | "test" => Layer::Tests,
        "infra" | "infrastructure" => Layer::Infra,
        _ => Layer::Unknown,
    }
}

/// Parse confidence from string
fn parse_confidence(s: &str) -> Confidence {
    match s.to_lowercase().as_str() {
        "high" => Confidence::High,
        "medium" | "med" => Confidence::Medium,
        _ => Confidence::Low,
    }
}

/// Apply LLM response to grouping
fn apply_response(
    grouping: &mut CodebaseGrouping,
    response: GroupingResponse,
) {
    for file_response in response.files {
        let path = PathBuf::from(&file_response.path);
        let new_layer = parse_layer(&file_response.layer);
        let confidence = parse_confidence(&file_response.confidence);
        
        // Get current layer
        let current_layer = grouping.get_layer(&path);
        
        // Reassign if layer changed
        if current_layer != Some(new_layer) {
            grouping.reassign_file(&path, new_layer, confidence);
        } else if let Some(assignment) = grouping.file_assignments.get_mut(&path) {
            // Update confidence even if layer didn't change
            assignment.confidence = confidence;
        }
        
        // Update feature if provided
        if let Some(feature_name) = file_response.feature {
            if !feature_name.is_empty() {
                // Remove from current feature and add to new one
                if let Some(group) = grouping.groups.get_mut(&new_layer) {
                    // Remove from any existing features in this group
                    for feature in &mut group.features {
                        feature.files.retain(|p| p != &path);
                    }
                    
                    // Remove from ungrouped
                    group.ungrouped_files.retain(|p| p != &path);
                    
                    // Add to new feature
                    if let Some(feature) = group.features.iter_mut().find(|f| f.name == feature_name) {
                        feature.files.push(path.clone());
                    } else {
                        let mut new_feature = Feature::new(&feature_name);
                        new_feature.files.push(path.clone());
                        group.features.push(new_feature);
                    }
                    
                    // Update assignment
                    if let Some(assignment) = grouping.file_assignments.get_mut(&path) {
                        assignment.feature = Some(feature_name);
                    }
                }
            }
        }
    }
    
    // Clean up empty features
    for group in grouping.groups.values_mut() {
        group.features.retain(|f| !f.files.is_empty());
    }
}

/// Enhance grouping using LLM for ambiguous files
/// 
/// Returns usage stats if LLM was called, None if no enhancement needed
pub async fn enhance_grouping(
    grouping: &mut CodebaseGrouping,
    index: &CodebaseIndex,
) -> anyhow::Result<Option<Usage>> {
    if !should_enhance(grouping) {
        return Ok(None);
    }
    
    let ambiguous_files = collect_ambiguous_files(grouping);
    if ambiguous_files.is_empty() {
        return Ok(None);
    }
    
    // Build file contexts
    let contexts: Vec<FileContext> = ambiguous_files.iter()
        .filter_map(|p| build_file_context(p, index, grouping))
        .take(MAX_BATCH_SIZE)
        .collect();
    
    if contexts.is_empty() {
        return Ok(None);
    }
    
    let system = build_system_prompt();
    let user = build_user_prompt(&contexts);
    
    // Call LLM with JSON mode
    let response = call_llm_json::<GroupingResponse>(system, &user).await?;
    
    // Apply response
    apply_response(grouping, response.0);
    
    // Mark as enhanced
    grouping.llm_enhanced = true;
    
    Ok(response.1)
}

/// Rate limit retry configuration
const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_SECS: u64 = 2;
const BACKOFF_MULTIPLIER: u64 = 2;

/// Internal function to call LLM with JSON response
/// Includes automatic retry with exponential backoff for rate limits
async fn call_llm_json<T: for<'de> Deserialize<'de>>(
    system: &str,
    user: &str,
) -> anyhow::Result<(T, Option<Usage>)> {
    const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

    let api_key = crate::config::Config::load()
        .get_api_key()
        .ok_or_else(|| anyhow::anyhow!("No API key configured. Run 'cosmos --setup' to get started."))?;
    
    let client = reqwest::Client::new();
    let url = OPENROUTER_URL;
    
    #[derive(serde::Serialize)]
    struct ChatRequest {
        model: String,
        messages: Vec<Message>,
        max_tokens: u32,
        stream: bool,
        response_format: ResponseFormat,
    }
    
    #[derive(serde::Serialize)]
    struct Message {
        role: String,
        content: String,
    }
    
    #[derive(serde::Serialize)]
    struct ResponseFormat {
        #[serde(rename = "type")]
        format_type: String,
    }
    
    #[derive(serde::Deserialize)]
    struct ChatResponse {
        choices: Vec<Choice>,
        usage: Option<Usage>,
    }
    
    #[derive(serde::Deserialize)]
    struct Choice {
        message: MessageContent,
    }
    
    #[derive(serde::Deserialize)]
    struct MessageContent {
        content: String,
    }
    
    let request = ChatRequest {
        model: Model::Speed.id().to_string(), // Use speed model for quick analysis
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
        max_tokens: 4096,
        stream: false,
        response_format: ResponseFormat {
            format_type: "json_object".to_string(),
        },
    };
    
    let mut last_error = String::new();
    let mut retry_count = 0;
    
    while retry_count <= MAX_RETRIES {
        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://cosmos.dev")
            .header("X-Title", "Cosmos")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request)
            .send()
            .await?;
        
        if response.status().is_success() {
            let chat_response: ChatResponse = response.json().await?;
            
            let content = chat_response
                .choices
                .first()
                .map(|c| c.message.content.clone())
                .ok_or_else(|| anyhow::anyhow!("No response from AI"))?;
            
            // Parse JSON response
            let parsed: T = serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse LLM response: {}", e))?;
            
            return Ok((parsed, chat_response.usage));
        }
        
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        
        // Handle rate limiting with retry
        if status.as_u16() == 429 && retry_count < MAX_RETRIES {
            retry_count += 1;
            let backoff_secs = INITIAL_BACKOFF_SECS * BACKOFF_MULTIPLIER.pow(retry_count - 1);
            
            last_error = format!(
                "Rate limited (attempt {}/{}). Retrying in {}s...",
                retry_count, MAX_RETRIES + 1, backoff_secs
            );
            
            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
            continue;
        }
        
        // Non-retryable error or max retries exceeded
        let error_msg = match status.as_u16() {
            401 => "Invalid API key. Run 'cosmos --setup' to update it.".to_string(),
            429 => format!(
                "Rate limited after {} retries. Try again in a few minutes.",
                retry_count
            ),
            _ => format!("API error {}: {}", status, text),
        };
        return Err(anyhow::anyhow!("{}", error_msg));
    }
    
    Err(anyhow::anyhow!("{}", last_error))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_layer() {
        assert_eq!(parse_layer("Frontend"), Layer::Frontend);
        assert_eq!(parse_layer("BACKEND"), Layer::Backend);
        assert_eq!(parse_layer("api"), Layer::API);
        assert_eq!(parse_layer("unknown"), Layer::Unknown);
    }
    
    #[test]
    fn test_parse_confidence() {
        assert_eq!(parse_confidence("high"), Confidence::High);
        assert_eq!(parse_confidence("medium"), Confidence::Medium);
        assert_eq!(parse_confidence("low"), Confidence::Low);
    }
}
