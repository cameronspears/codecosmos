use crate::app::background;
use crate::app::messages::BackgroundMessage;
use crate::app::RuntimeContext;
use crate::suggest;
use crate::ui::{App, LoadingState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in question (ask cosmos) mode
pub(super) fn handle_question_input(
    app: &mut App,
    key: KeyEvent,
    ctx: &RuntimeContext,
) -> Result<()> {
    match key.code {
        KeyCode::Esc => app.exit_question(),
        KeyCode::Up if app.question_input.is_empty() => app.question_suggestion_up(),
        KeyCode::Down if app.question_input.is_empty() => app.question_suggestion_down(),
        KeyCode::Tab => app.use_selected_suggestion(),
        KeyCode::Enter => submit_question(app, ctx)?,
        KeyCode::Backspace => app.question_pop(),
        KeyCode::Char(c) => app.question_push(c),
        _ => {}
    }
    Ok(())
}

/// Compute a context hash for cache validation
/// Uses file count and suggestion count as a simple indicator of codebase state
fn compute_context_hash(app: &App) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    app.index.files.len().hash(&mut hasher);
    app.suggestions.suggestions.len().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Submit a question to the LLM
fn submit_question(app: &mut App, ctx: &RuntimeContext) -> Result<()> {
    // If input is empty, use the selected suggestion first
    if app.question_input.is_empty() && !app.question_suggestions.is_empty() {
        app.use_selected_suggestion();
    }
    let question = app.take_question();
    if question.is_empty() {
        return Ok(());
    }

    // Check cache first
    let context_hash = compute_context_hash(app);
    if let Some(cached_answer) = app.question_cache.get(&question, &context_hash) {
        // Cache hit! Use cached answer directly
        let _ = ctx.tx.send(BackgroundMessage::QuestionResponse {
            answer: cached_answer.to_string(),
            usage: None, // No usage for cached response
        });
        return Ok(());
    }

    // Cache miss - send question to LLM
    let index_clone = ctx.index.clone();
    let context_clone = app.context.clone();
    let tx_question = ctx.tx.clone();
    let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
    let question_for_cache = question.clone();
    let context_hash_for_cache = context_hash;

    app.loading = LoadingState::Answering;

    background::spawn_background(ctx.tx.clone(), "ask_question", async move {
        let mem = if repo_memory_context.trim().is_empty() {
            None
        } else {
            Some(repo_memory_context)
        };
        match suggest::llm::ask_question(&index_clone, &context_clone, &question, mem).await {
            Ok((answer, usage)) => {
                // Send response with cache metadata for storage
                let _ = tx_question.send(BackgroundMessage::QuestionResponseWithCache {
                    question: question_for_cache,
                    answer,
                    usage,
                    context_hash: context_hash_for_cache,
                });
            }
            Err(e) => {
                let _ = tx_question.send(BackgroundMessage::Error(e.to_string()));
            }
        }
    });
    Ok(())
}
