//! Cosmos - A contemplative vibe coding companion
//!
//! C O S M O S
//!
//! An AI-powered IDE in the terminal that uses codebase indexing
//! to suggest improvements, bug fixes, and optimizations.

mod cache;
mod config;
mod context;
mod grouping;
mod history;
mod index;
mod onboarding;
mod safe_apply;
mod suggest;
mod ui;

// Keep these for compatibility during transition
mod git_ops;

use anyhow::Result;
use clap::Parser;
use context::WorkContext;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use index::CodebaseIndex;
use ratatui::prelude::*;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use suggest::SuggestionEngine;
use ui::{ActivePanel, App, InputMode, LoadingState, Overlay, WorkflowStep};

#[derive(Parser, Debug)]
#[command(
    name = "cosmos",
    about = "A contemplative vibe coding companion",
    long_about = "C O S M O S\n\n\
                  A contemplative companion for your codebase.\n\n\
                  Uses AST-based indexing and AI to suggest improvements,\n\
                  bug fixes, features, and optimizations.",
    version
)]
struct Args {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Set up OpenRouter API key for AI features (BYOK mode)
    #[arg(long)]
    setup: bool,

    /// Show stats and exit (no TUI)
    #[arg(long)]
    stats: bool,

    
}

/// Messages from background tasks to the main UI thread
pub enum BackgroundMessage {
    SuggestionsReady {
        suggestions: Vec<suggest::Suggestion>,
        usage: Option<suggest::llm::Usage>,
        model: String,
    },
    SuggestionsError(String),
    SummariesReady {
        summaries: std::collections::HashMap<PathBuf, String>,
        usage: Option<suggest::llm::Usage>,
    },
    /// Incremental summary progress update
    SummaryProgress {
        completed: usize,
        total: usize,
        summaries: std::collections::HashMap<PathBuf, String>,
    },
    SummariesError(String),
    /// Quick preview ready (Phase 1 - fast)
    PreviewReady {
        suggestion_id: uuid::Uuid,
        file_path: PathBuf,
        summary: String,
        preview: suggest::llm::FixPreview,
    },
    PreviewError(String),
    /// Direct fix applied (Smart preset generated + applied the change)
    DirectFixApplied {
        suggestion_id: uuid::Uuid,
        file_path: PathBuf,
        description: String,
        modified_areas: Vec<String>,
        backup_path: PathBuf,
        safety_checks: Vec<safe_apply::CheckResult>,
        usage: Option<suggest::llm::Usage>,
        branch_name: String,
    },
    DirectFixError(String),
    /// Ship workflow progress update
    ShipProgress(ui::ShipStep),
    /// Ship workflow completed successfully with PR URL
    ShipComplete(String),
    /// Ship workflow error
    ShipError(String),
    /// AI code review completed
    ReviewReady(Vec<ui::PRReviewComment>),
    /// PR created successfully
    PRCreated(String), // PR URL
    /// Generic error (used for push/etc)
    Error(String),
    /// Response to a user question
    QuestionResponse {
        question: String,
        answer: String,
        usage: Option<suggest::llm::Usage>,
    },
    /// Verification review completed (adversarial review of applied changes)
    VerificationComplete {
        findings: Vec<suggest::llm::ReviewFinding>,
        summary: String,
        usage: Option<suggest::llm::Usage>,
    },
    /// Verification fix completed (Smart fixed the selected findings)
    VerificationFixComplete {
        new_content: String,
        description: String,
        usage: Option<suggest::llm::Usage>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {

    let args = Args::parse();

    // Handle --setup flag (BYOK mode)
    if args.setup {
        return setup_api_key();
    }

    // Check for first run and show onboarding
    if onboarding::is_first_run() {
        match onboarding::run_onboarding() {
            Ok(true) => {
                // Setup completed, continue to TUI
                eprintln!();
            }
            Ok(false) => {
                // Setup skipped, continue to TUI
                eprintln!();
            }
            Err(e) => {
                eprintln!("  Onboarding error: {}", e);
                eprintln!("  Continuing without setup...");
            }
        }
    }

    let path = args.path.canonicalize()?;

    // Initialize cache
    let cache_manager = cache::Cache::new(&path);
    
    // Initialize index (fast, synchronous)
    let index = init_index(&path, &cache_manager)?;
    let context = init_context(&path)?;
    
    // Create empty suggestion engine (will be populated by LLM)
    let suggestions = SuggestionEngine::new_empty(index.clone());

    // Stats mode: print and exit
    if args.stats {
        print_stats(&index, &suggestions, &context);
        return Ok(());
    }

    // Run TUI with background LLM tasks
    run_tui(index, suggestions, context, cache_manager, path).await
}

/// Initialize the codebase index
fn init_index(path: &PathBuf, cache_manager: &cache::Cache) -> Result<CodebaseIndex> {
    eprint!("  Indexing codebase...");
    
    let index = CodebaseIndex::new(path)?;
    let stats = index.stats();
    
    // Save index cache
    let index_cache = cache::IndexCache::from_index(&index);
    let _ = cache_manager.save_index_cache(&index_cache);
    
    eprintln!(
        " {} files, {} symbols",
        stats.file_count,
        stats.symbol_count
    );
    
    Ok(index)
}

/// Initialize the work context
fn init_context(path: &PathBuf) -> Result<WorkContext> {
    eprint!("  Loading context...");
    
    let context = WorkContext::load(path)?;
    
    eprintln!(
        " {} on {}, {} changed",
        context.branch,
        if context.inferred_focus.is_some() { 
            context.inferred_focus.as_ref().unwrap() 
        } else { 
            "project" 
        },
        context.modified_count
    );
    
    Ok(context)
}

/// Print stats and exit
fn print_stats(index: &CodebaseIndex, suggestions: &SuggestionEngine, context: &WorkContext) {
    let stats = index.stats();
    let counts = suggestions.counts();

    println!();
    println!("  ╔══════════════════════════════════════════════════╗");
    println!("  ║             C O S M O S   Stats                  ║");
    println!("  ╠══════════════════════════════════════════════════╣");
    println!("  ║                                                  ║");
    println!("  ║  Files:     {:>6}                               ║", stats.file_count);
    println!("  ║  LOC:       {:>6}                               ║", stats.total_loc);
    println!("  ║  Symbols:   {:>6}                               ║", stats.symbol_count);
    println!("  ║  Patterns:  {:>6}                               ║", stats.pattern_count);
    println!("  ║                                                  ║");
    println!("  ║  Suggestions:                                    ║");
    println!("  ║    High:    {:>6} ●                             ║", counts.high);
    println!("  ║    Medium:  {:>6} ◐                             ║", counts.medium);
    println!("  ║    Low:     {:>6} ○                             ║", counts.low);
    println!("  ║                                                  ║");
    println!("  ║  Context:                                        ║");
    println!("  ║    Branch:  {:>20}               ║", truncate(&context.branch, 20));
    println!("  ║    Changed: {:>6}                               ║", context.modified_count);
    println!("  ║                                                  ║");
    println!("  ╚══════════════════════════════════════════════════╝");
    println!();

    // Top suggestions
    let top = suggestions.high_priority_suggestions();
    if !top.is_empty() {
        println!("  Top suggestions:");
        println!();
        for (i, s) in top.iter().take(5).enumerate() {
            println!("    {}. {} {}: {}", 
                i + 1,
                s.priority.icon(),
                s.kind.label(),
                truncate(&s.summary, 50)
            );
        }
        println!();
    }
}

/// Set up the API key interactively
fn setup_api_key() -> Result<()> {
    config::setup_api_key_interactive()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    println!("  + API key configured. You can now use AI features!");
    Ok(())
}

/// Run the TUI application with background LLM tasks
async fn run_tui(
    index: CodebaseIndex,
    suggestions: SuggestionEngine,
    context: WorkContext,
    cache_manager: cache::Cache,
    repo_path: PathBuf,
) -> Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app with loading state
    let mut app = App::new(index.clone(), suggestions, context.clone());
    // Load repo-local “memory” (decisions/conventions) from .cosmos/
    app.repo_memory = cache_manager.load_repo_memory();
    // Load cached domain glossary (auto-extracted terminology)
    app.glossary = cache_manager.load_glossary().unwrap_or_default();
    
    // Check if we have API access (and budgets allow it)
    let mut ai_enabled = suggest::llm::is_available();
    if ai_enabled {
        if let Err(e) = app.config.allow_ai(0.0) {
            ai_enabled = false;
            app.show_toast(&e);
        }
    }
    
    // ═══════════════════════════════════════════════════════════════════════
    //  SMART SUMMARY CACHING
    // ═══════════════════════════════════════════════════════════════════════
    
    // Compute file hashes for change detection
    let file_hashes = cache::compute_file_hashes(&index);
    
    // Load cached LLM summaries and apply immediately
    let mut llm_cache = cache_manager.load_llm_summaries_cache()
        .unwrap_or_else(cache::LlmSummaryCache::new);
    if llm_cache.normalize_paths(&index.root) {
        let _ = cache_manager.save_llm_summaries_cache(&llm_cache);
    }
    
    // Get all valid cached summaries and load them immediately (instant startup!)
    let cached_summaries = llm_cache.get_all_valid_summaries(&file_hashes);
    let cached_count = cached_summaries.len();
    let total_files = file_hashes.len();
    
    if !cached_summaries.is_empty() {
        app.update_summaries(cached_summaries);
        eprintln!("  Loaded {} cached summaries ({} files total)", cached_count, total_files);
    }
    
    // Discover project context (for better quality summaries)
    let project_context = suggest::llm::discover_project_context(&index);
    llm_cache.set_project_context(project_context.clone());
    
    // Find files that need new/updated summaries
    let mut files_needing_summary = llm_cache.get_files_needing_summary(&file_hashes);

    // Optional privacy/cost control: only summarize changed files (and their immediate blast radius)
    if app.config.summarize_changed_only {
        let changed: std::collections::HashSet<PathBuf> = context
            .all_changed_files()
            .into_iter()
            .cloned()
            .collect();
        let mut wanted = changed.clone();
        for c in &changed {
            if let Some(file_index) = index.files.get(c) {
                for u in &file_index.summary.used_by {
                    wanted.insert(u.clone());
                }
                for d in &file_index.summary.depends_on {
                    wanted.insert(d.clone());
                }
            }
        }
        files_needing_summary.retain(|p| wanted.contains(p));
    }
    let needs_summary_count = files_needing_summary.len();
    
    // Track if we need to generate summaries (used to control loading state)
    app.needs_summary_generation = needs_summary_count > 0;
    
    if needs_summary_count > 0 {
        eprintln!("  {} files need summary generation", needs_summary_count);
    } else if cached_count > 0 {
        eprintln!("  All {} summaries loaded from cache", cached_count);
    }
    
    eprintln!();

    // Create channel for background tasks
    let (tx, rx) = mpsc::channel::<BackgroundMessage>();

    // ═══════════════════════════════════════════════════════════════════════
    //  SEQUENTIAL INIT: Summaries first (builds glossary), then suggestions
    // ═══════════════════════════════════════════════════════════════════════
    
    if ai_enabled {
        if !files_needing_summary.is_empty() {
            // Phase 1: Summaries needed - generate them first, suggestions come after
            app.loading = LoadingState::GeneratingSummaries;
            app.pending_suggestions_on_init = true;
            app.summary_progress = Some((0, needs_summary_count));
            
            let index_clone2 = index.clone();
            let context_clone2 = context.clone();
            let tx_summaries = tx.clone();
            let cache_path = repo_path.clone();
            let file_hashes_clone = file_hashes.clone();
            
            // Prioritize files for generation
            let (high_priority, medium_priority, low_priority) = 
                suggest::llm::prioritize_files_for_summary(&index_clone2, &context_clone2, &files_needing_summary);
            
            // Show initial cached count
            if cached_count > 0 {
                app.show_toast(&format!("{}/{} cached · summarizing {}", cached_count, total_files, needs_summary_count));
            }
            
            // Calculate total file count for progress
            let total_to_process = high_priority.len() + medium_priority.len() + low_priority.len();
            
            tokio::spawn(async move {
                let cache = cache::Cache::new(&cache_path);
                
                // Load existing cache to update incrementally
                let mut llm_cache = cache.load_llm_summaries_cache()
                    .unwrap_or_else(cache::LlmSummaryCache::new);
                
                // Load existing glossary to merge new terms into
                let mut glossary = cache.load_glossary()
                    .unwrap_or_else(cache::DomainGlossary::new);
                
                let mut all_summaries = HashMap::new();
                let mut total_usage = suggest::llm::Usage::default();
                let mut completed_count = 0usize;
                
                // Process all priority tiers with parallel batching within each tier
                let priority_tiers = [
                    ("high", high_priority),
                    ("medium", medium_priority), 
                    ("low", low_priority),
                ];
                
                for (_tier_name, files) in priority_tiers {
                    if files.is_empty() {
                        continue;
                    }
                    
                    // Use large batch size (16 files) for faster processing
                    let batch_size = 16;
                    let batches: Vec<_> = files.chunks(batch_size).collect();
                    
                    // Process batches sequentially (llm.rs handles internal parallelism)
                    for batch in batches {
                        if let Ok((summaries, batch_glossary, usage)) = suggest::llm::generate_summaries_for_files(
                            &index_clone2, batch, &project_context
                        ).await {
                            // Update cache with new summaries
                            for (path, summary) in &summaries {
                                if let Some(hash) = file_hashes_clone.get(path) {
                                    llm_cache.set_summary(path.clone(), summary.clone(), hash.clone());
                                }
                            }
                            // Merge new terms into glossary
                            glossary.merge(&batch_glossary);
                            
                            // Save cache incrementally after each batch
                            let _ = cache.save_llm_summaries_cache(&llm_cache);
                            let _ = cache.save_glossary(&glossary);
                            
                            completed_count += summaries.len();
                            
                            // Send progress update with new summaries
                            let _ = tx_summaries.send(BackgroundMessage::SummaryProgress {
                                completed: completed_count,
                                total: total_to_process,
                                summaries: summaries.clone(),
                            });
                            
                            all_summaries.extend(summaries);
                            if let Some(u) = usage {
                                total_usage.prompt_tokens += u.prompt_tokens;
                                total_usage.completion_tokens += u.completion_tokens;
                                total_usage.total_tokens += u.total_tokens;
                            }
                        }
                    }
                }
                
                let final_usage = if total_usage.total_tokens > 0 {
                    Some(total_usage)
                } else {
                    None
                };
                
                // Send final message (summaries already sent via progress, so send empty)
                let _ = tx_summaries.send(BackgroundMessage::SummariesReady { 
                    summaries: HashMap::new(), 
                    usage: final_usage 
                });
            });
        } else {
            // Phase 2 only: All summaries cached - generate suggestions directly with cached glossary
            app.loading = LoadingState::GeneratingSuggestions;
            
            let index_clone = index.clone();
            let context_clone = context.clone();
            let tx_suggestions = tx.clone();
            let cache_clone_path = repo_path.clone();
            let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
            let glossary_clone = app.glossary.clone();
            
            if !glossary_clone.is_empty() {
                app.show_toast(&format!("{} glossary terms · generating suggestions", glossary_clone.len()));
            }
            
            tokio::spawn(async move {
                let mem = if repo_memory_context.trim().is_empty() {
                    None
                } else {
                    Some(repo_memory_context)
                };
                let glossary_ref = if glossary_clone.is_empty() { None } else { Some(&glossary_clone) };
                match suggest::llm::analyze_codebase(&index_clone, &context_clone, mem, glossary_ref).await {
                    Ok((suggestions, usage)) => {
                        // Cache the suggestions
                        let cache = cache::Cache::new(&cache_clone_path);
                        let cache_data = cache::SuggestionsCache::from_suggestions(&suggestions);
                        let _ = cache.save_suggestions_cache(&cache_data);
                        
                        let _ = tx_suggestions.send(BackgroundMessage::SuggestionsReady {
                            suggestions,
                            usage,
                            model: "smart".to_string(),
                        });
                    }
                    Err(e) => {
                        let _ = tx_suggestions.send(BackgroundMessage::SuggestionsError(e.to_string()));
                    }
                }
            });
        }
    }

    // Main loop with async event handling
    let result = run_loop(&mut terminal, &mut app, rx, tx, repo_path, index);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main event loop with background message handling
fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rx: mpsc::Receiver<BackgroundMessage>,
    tx: mpsc::Sender<BackgroundMessage>,
    repo_path: PathBuf,
    index: CodebaseIndex,
) -> Result<()> {
    // Track last git status refresh time
    let mut last_git_refresh = std::time::Instant::now();
    const GIT_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
    
    loop {
        // Clear expired toasts
        app.clear_expired_toast();

        // Advance spinner animation
        app.tick_loading();
        
        // Periodically refresh git status (every 2 seconds)
        if last_git_refresh.elapsed() >= GIT_REFRESH_INTERVAL {
            let _ = app.context.refresh();
            last_git_refresh = std::time::Instant::now();
        }

        // Check for background messages (non-blocking)
        while let Ok(msg) = rx.try_recv() {
            match msg {
                BackgroundMessage::SuggestionsReady { suggestions, usage, model } => {
                    let count = suggestions.len();
                    for s in suggestions {
                        app.suggestions.add_llm_suggestion(s);
                    }

                    // Diff-first ordering: changed files and their blast radius float to the top.
app.suggestions.sort_with_context(&app.context);

                    // Track cost (Smart preset for suggestions)
                    if let Some(u) = usage {
                        let cost = u.calculate_cost(suggest::llm::Model::Smart);
                        app.session_cost += cost;
                        app.session_tokens += u.total_tokens;
                        let _ = app.config.record_tokens(u.total_tokens);
                        let _ = app.config.allow_ai(app.session_cost).map_err(|e| app.show_toast(&e));
                    }
                    
                    // If summaries are still generating, switch to that loading state
                    // Otherwise, clear loading
                    if app.needs_summary_generation && app.summary_progress.is_some() {
                        app.loading = LoadingState::GeneratingSummaries;
                    } else {
                        app.loading = LoadingState::None;
                    }
                    
                    // More prominent toast for suggestions
                    app.show_toast(&format!("{} suggestions ready ({})", count, &model));
                    app.active_model = Some(model);
                }
                BackgroundMessage::SuggestionsError(e) => {
                    // If summaries are still generating, switch to that loading state
                    if app.needs_summary_generation && app.summary_progress.is_some() {
                        app.loading = LoadingState::GeneratingSummaries;
                    } else {
                        app.loading = LoadingState::None;
                    }
                    app.show_toast(&format!("Error: {}", truncate(&e, 80)));
                }
                BackgroundMessage::SummariesReady { summaries, usage } => {
                    let new_count = summaries.len();
                    app.update_summaries(summaries);

                    // Track cost (using Speed preset for summaries)
                    if let Some(u) = usage {
                        let cost = u.calculate_cost(suggest::llm::Model::Speed);
                        app.session_cost += cost;
                        app.session_tokens += u.total_tokens;
                        let _ = app.config.record_tokens(u.total_tokens);
                        let _ = app.config.allow_ai(app.session_cost).map_err(|e| app.show_toast(&e));
                    }
                    
                    // Reload glossary from cache (it was built during summary generation)
                    let cache = cache::Cache::new(&repo_path);
                    if let Some(new_glossary) = cache.load_glossary() {
                        app.glossary = new_glossary;
                    }
                    
                    app.summary_progress = None;
                    app.needs_summary_generation = false;
                    
                    // If we're waiting to generate suggestions after reset, do it now
                    if app.pending_suggestions_on_init {
                        app.pending_suggestions_on_init = false;
                        
                        // Check if AI is still available
                        let ai_enabled = suggest::llm::is_available() && app.config.allow_ai(app.session_cost).is_ok();
                        
                        if ai_enabled {
                            let index_clone = app.index.clone();
                            let context_clone = app.context.clone();
                            let tx_suggestions = tx.clone();
                            let cache_clone_path = repo_path.clone();
                            let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                            let glossary_clone = app.glossary.clone();
                            
                            app.loading = LoadingState::GeneratingSuggestions;
                            app.show_toast(&format!("{} terms in glossary · generating suggestions...", glossary_clone.len()));
                            
                            tokio::spawn(async move {
                                let mem = if repo_memory_context.trim().is_empty() {
                                    None
                                } else {
                                    Some(repo_memory_context)
                                };
                                let glossary_ref = if glossary_clone.is_empty() { None } else { Some(&glossary_clone) };
                                match suggest::llm::analyze_codebase(&index_clone, &context_clone, mem, glossary_ref).await {
                                    Ok((suggestions, usage)) => {
                                        // Cache the suggestions
                                        let cache = cache::Cache::new(&cache_clone_path);
                                        let cache_data = cache::SuggestionsCache::from_suggestions(&suggestions);
                                        let _ = cache.save_suggestions_cache(&cache_data);
                                        
                                        let _ = tx_suggestions.send(BackgroundMessage::SuggestionsReady {
                                            suggestions,
                                            usage,
                                            model: "smart".to_string(),
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx_suggestions.send(BackgroundMessage::SuggestionsError(e.to_string()));
                                    }
                                }
                            });
                        } else {
                            app.loading = LoadingState::None;
                            if new_count > 0 {
                                app.show_toast(&format!("{} summaries · {} glossary terms", new_count, app.glossary.len()));
                            }
                        }
                    } else {
                        // Not waiting for suggestions, just finish up
                        if !matches!(app.loading, LoadingState::GeneratingSuggestions) {
                            app.loading = LoadingState::None;
                        }
                        if new_count > 0 {
                            app.show_toast(&format!("{} summaries · {} glossary terms", new_count, app.glossary.len()));
                        } else {
                            app.show_toast(&format!("Summaries ready · {} glossary terms", app.glossary.len()));
                        }
                    }
                }
                BackgroundMessage::SummaryProgress { completed, total, summaries } => {
                    // Merge new summaries as they arrive
                    app.update_summaries(summaries);
                    // Track progress for display
                    app.summary_progress = Some((completed, total));
                }
                BackgroundMessage::SummariesError(e) => {
                    // Only clear loading if we're not still waiting for suggestions
                    if !matches!(app.loading, LoadingState::GeneratingSuggestions) {
                        app.loading = LoadingState::None;
                    }
                    app.summary_progress = None;
                    app.show_toast(&format!("Summary error: {}", truncate(&e, 80)));
                }
                BackgroundMessage::PreviewReady { suggestion_id, file_path, summary, preview } => {
                    app.loading = LoadingState::None;
                    // Check if we're in the new workflow or legacy overlay mode
                    if app.workflow_step == WorkflowStep::Verify {
                        app.set_verify_preview(preview);
                    } else {
                        // Legacy: show overlay
                        app.show_fix_preview(suggestion_id, file_path, summary, preview);
                    }
                }
                BackgroundMessage::PreviewError(e) => {
                    app.loading = LoadingState::None;
                    // Reset workflow if we were in Verify step
                    if app.workflow_step == WorkflowStep::Verify {
                        app.workflow_step = WorkflowStep::Suggestions;
                        app.verify_state = ui::VerifyState::default();
                    }
                    app.show_toast(&format!("Preview error: {}", truncate(&e, 80)));
                }
                BackgroundMessage::DirectFixApplied { suggestion_id, file_path, description, modified_areas, backup_path, safety_checks: _, usage, branch_name } => {
                    // Track cost
                    if let Some(u) = usage {
                        let cost = u.calculate_cost(suggest::llm::Model::Smart);
                        app.session_cost += cost;
                        app.session_tokens += u.total_tokens;
                        let _ = app.config.record_tokens(u.total_tokens);
                        let _ = app.config.allow_ai(app.session_cost).map_err(|e| app.show_toast(&e));
                    }

                    app.loading = LoadingState::None;
                    app.suggestions.mark_applied(suggestion_id);

                    // Store the cosmos branch name - this enables the Ship workflow
                    app.cosmos_branch = Some(branch_name.clone());

                    // Track as pending change for batch commit
                    let diff = format!("Modified areas: {}", modified_areas.join(", "));
                    app.add_pending_change(suggestion_id, file_path.clone(), description.clone(), diff, backup_path.clone());

                    // Read original (backup) and new content for verification
                    let original_content = std::fs::read_to_string(&backup_path).unwrap_or_default();
                    let full_path = app.repo_path.join(&file_path);
                    let new_content = std::fs::read_to_string(&full_path).unwrap_or_default();

                    // Use new workflow Review step if in workflow mode
                    if app.workflow_step == WorkflowStep::Verify {
                        app.start_review(file_path.clone(), original_content.clone(), new_content.clone());
                        
                        // Trigger verification in background
                        let tx_verify = tx.clone();
                        let file_for_verify = file_path.clone();
                        tokio::spawn(async move {
                            let files_with_content = vec![(file_for_verify, original_content, new_content)];
                            match suggest::llm::verify_changes(&files_with_content, 1, &[]).await {
                                Ok(review) => {
                                    let _ = tx_verify.send(BackgroundMessage::VerificationComplete {
                                        findings: review.findings,
                                        summary: review.summary,
                                        usage: review.usage,
                                    });
                                }
                                Err(e) => {
                                    let _ = tx_verify.send(BackgroundMessage::Error(format!("Verification failed: {}", e)));
                                }
                            }
                        });
                    } else {
                        // Legacy: show verification review overlay
                        app.show_verification_review(file_path.clone(), original_content.clone(), new_content.clone());

                        // Trigger verification in background
                        let tx_verify = tx.clone();
                        let file_for_verify = file_path.clone();
                        tokio::spawn(async move {
                            let files_with_content = vec![(file_for_verify, original_content, new_content)];
                            match suggest::llm::verify_changes(&files_with_content, 1, &[]).await {
                                Ok(review) => {
                                    let _ = tx_verify.send(BackgroundMessage::VerificationComplete {
                                        findings: review.findings,
                                        summary: review.summary,
                                        usage: review.usage,
                                    });
                                }
                                Err(e) => {
                                    let _ = tx_verify.send(BackgroundMessage::Error(format!("Verification failed: {}", e)));
                                }
                            }
                        });
                    }
                }
                BackgroundMessage::DirectFixError(e) => {
                    app.loading = LoadingState::None;
                    // Reset workflow if we were in Verify step
                    if app.workflow_step == WorkflowStep::Verify {
                        app.workflow_step = WorkflowStep::Suggestions;
                        app.verify_state = ui::VerifyState::default();
                    }
                    app.show_toast(&format!("Apply failed: {}", truncate(&e, 80)));
                }
                BackgroundMessage::ShipProgress(step) => {
                    // Handle workflow mode
                    if app.workflow_step == WorkflowStep::Ship {
                        app.set_ship_step(step);
                    } else {
                        app.update_ship_step(step);
                        app.ship_step = Some(step);
                    }
                }
                BackgroundMessage::ShipComplete(url) => {
                    // Handle workflow mode
                    if app.workflow_step == WorkflowStep::Ship {
                        app.set_ship_pr_url(url.clone());
                        app.show_toast("PR created!");
                    } else {
                        app.update_ship_step(ui::ShipStep::Done);
                        app.ship_step = Some(ui::ShipStep::Done);
                        app.pr_url = Some(url.clone());
                        app.clear_pending_changes();
                    }
                }
                BackgroundMessage::ShipError(e) => {
                    app.ship_step = None;
                    app.close_overlay();
                    app.show_toast(&format!("Ship failed: {}", truncate(&e, 80)));
                }
                BackgroundMessage::ReviewReady(comments) => {
                    app.set_review_comments(comments);
                }
                BackgroundMessage::PRCreated(url) => {
                    app.set_pr_url(url.clone());
                }
                BackgroundMessage::Error(e) => {
                    app.loading = LoadingState::None;
                    app.show_toast(&truncate(&e, 100));
                }
                BackgroundMessage::QuestionResponse { question, answer, usage } => {
                    // Track cost
                    if let Some(u) = usage {
                        let cost = u.calculate_cost(suggest::llm::Model::Speed);
                        app.session_cost += cost;
                        app.session_tokens += u.total_tokens;
                        let _ = app.config.record_tokens(u.total_tokens);
                        let _ = app.config.allow_ai(app.session_cost).map_err(|e| app.show_toast(&e));
                    }

                    app.loading = LoadingState::None;
                    // Show the response in the inquiry overlay
                    let response = format!("Q: {}\n\n{}", question, answer);
                    app.show_inquiry(response);
                }
                BackgroundMessage::VerificationComplete { findings, summary, usage } => {
                    // Track cost
                    if let Some(u) = usage {
                        let cost = u.calculate_cost(suggest::llm::Model::Reviewer);
                        app.session_cost += cost;
                        app.session_tokens += u.total_tokens;
                        let _ = app.config.record_tokens(u.total_tokens);
                    }
                    // Use new workflow or legacy overlay
                    if app.workflow_step == WorkflowStep::Review {
                        app.set_review_findings(findings, summary);
                    } else {
                        app.set_verification_findings(findings, summary);
                    }
                }
                BackgroundMessage::VerificationFixComplete { new_content, description, usage } => {
                    // Track cost
                    if let Some(u) = usage {
                        let cost = u.calculate_cost(suggest::llm::Model::Smart);
                        app.session_cost += cost;
                        app.session_tokens += u.total_tokens;
                        let _ = app.config.record_tokens(u.total_tokens);
                    }
                    
                    app.show_toast(&format!("Fixed: {}", truncate(&description, 40)));
                    
                    // Handle based on workflow mode
                    if app.workflow_step == WorkflowStep::Review {
                        // Update workflow review state
                        let file_path = app.review_state.file_path.clone();
                        let original_content = app.review_state.original_content.clone();
                        let iteration = app.review_state.review_iteration + 1;
                        let fixed_titles = app.review_state.fixed_titles.clone();
                        
                        app.review_fix_complete(new_content.clone());
                        
                        // Trigger re-review
                        if let Some(fp) = file_path {
                            app.review_state.reviewing = true;
                            app.loading = LoadingState::ReviewingChanges;
                            
                            let tx_verify = tx.clone();
                            tokio::spawn(async move {
                                let files_with_content = vec![(fp, original_content, new_content)];
                                match suggest::llm::verify_changes(&files_with_content, iteration, &fixed_titles).await {
                                    Ok(review) => {
                                        let _ = tx_verify.send(BackgroundMessage::VerificationComplete {
                                            findings: review.findings,
                                            summary: review.summary,
                                            usage: review.usage,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx_verify.send(BackgroundMessage::Error(format!("Re-verification failed: {}", e)));
                                    }
                                }
                            });
                        }
                    } else {
                        // Legacy overlay mode
                        app.update_verification_content(new_content.clone());
                        
                        if let Some((file_path, original_content, _, iteration, fixed_titles)) = app.get_verification_state() {
                            if let Overlay::VerificationReview { reviewing, .. } = &mut app.overlay {
                                *reviewing = true;
                            }
                            app.loading = LoadingState::ReviewingChanges;
                            
                            let tx_verify = tx.clone();
                            tokio::spawn(async move {
                                let files_with_content = vec![(file_path, original_content, new_content)];
                                match suggest::llm::verify_changes(&files_with_content, iteration, &fixed_titles).await {
                                    Ok(review) => {
                                        let _ = tx_verify.send(BackgroundMessage::VerificationComplete {
                                            findings: review.findings,
                                            summary: review.summary,
                                            usage: review.usage,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx_verify.send(BackgroundMessage::Error(format!("Re-verification failed: {}", e)));
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }

        // Render
        terminal.draw(|f| ui::render(f, app))?;

        // Poll for events with fast timeout (snappy animations)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Handle search input mode
                if app.input_mode == InputMode::Search {
                    match key.code {
                        KeyCode::Esc => app.exit_search(),
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Backspace => app.search_pop(),
                        KeyCode::Char(c) => app.search_push(c),
                        _ => {}
                    }
                    continue;
                }
                
                // Handle question input mode
                if app.input_mode == InputMode::Question {
                    match key.code {
                        KeyCode::Esc => app.exit_question(),
                        KeyCode::Enter => {
                            let question = app.take_question();
                            if !question.is_empty() {
                                // Privacy preview (what will be sent) before the network call
                                if app.config.privacy_preview {
                                    app.show_inquiry_preview(question);
                                } else {
                                    if let Err(e) = app.config.allow_ai(app.session_cost) {
                                        app.show_toast(&e);
                                        continue;
                                    }
                                    // Send question to LLM
                                    let index_clone = index.clone();
                                    let context_clone = app.context.clone();
                                    let tx_question = tx.clone();
                                    let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                    
                                    app.loading = LoadingState::Answering;
                                    
                                    tokio::spawn(async move {
                                        let mem = if repo_memory_context.trim().is_empty() {
                                            None
                                        } else {
                                            Some(repo_memory_context)
                                        };
                                        match suggest::llm::ask_question(&index_clone, &context_clone, &question, mem).await {
                                            Ok((answer, usage)) => {
                                                let _ = tx_question.send(BackgroundMessage::QuestionResponse {
                                                    question,
                                                    answer,
                                                    usage,
                                                });
                                            }
                                            Err(e) => {
                                                let _ = tx_question.send(BackgroundMessage::Error(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        KeyCode::Backspace => app.question_pop(),
                        KeyCode::Char(c) => app.question_push(c),
                        _ => {}
                    }
                    continue;
                }

                // Handle overlay mode
                if app.overlay != Overlay::None {
                    // Inquiry privacy preview overlay
                    if let Overlay::InquiryPreview { question, .. } = &app.overlay {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                            KeyCode::Down => app.overlay_scroll_down(),
                            KeyCode::Up => app.overlay_scroll_up(),
                            KeyCode::Enter => {
                                if let Err(e) = app.config.allow_ai(app.session_cost) {
                                    app.show_toast(&e);
                                    continue;
                                }
                                let question = question.clone();
                                let index_clone = index.clone();
                                let context_clone = app.context.clone();
                                let tx_question = tx.clone();
                                let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                app.loading = LoadingState::Answering;
                                app.close_overlay();
                                tokio::spawn(async move {
                                    let mem = if repo_memory_context.trim().is_empty() {
                                        None
                                    } else {
                                        Some(repo_memory_context)
                                    };
                                    match suggest::llm::ask_question(&index_clone, &context_clone, &question, mem).await {
                                        Ok((answer, usage)) => {
                                            let _ = tx_question.send(BackgroundMessage::QuestionResponse {
                                                question,
                                                answer,
                                                usage,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = tx_question.send(BackgroundMessage::Error(e.to_string()));
                                        }
                                    }
                                });
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Repo memory overlay
                    if let Overlay::RepoMemory { mode, .. } = &app.overlay {
                        match *mode {
                            ui::RepoMemoryMode::Add => match key.code {
                                KeyCode::Esc => app.memory_cancel_add(),
                                KeyCode::Enter => match app.memory_commit_add() {
                                    Ok(()) => app.show_toast("Saved to repo memory"),
                                    Err(e) => app.show_toast(&e),
                                },
                                KeyCode::Backspace => app.memory_input_pop(),
                                KeyCode::Char(c) => app.memory_input_push(c),
                                _ => {}
                            },
                            ui::RepoMemoryMode::View => match key.code {
                                KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                                KeyCode::Down => app.memory_move(1),
                                KeyCode::Up => app.memory_move(-1),
                                KeyCode::Char('a') => app.memory_start_add(),
                                _ => {}
                            },
                        }
                        continue;
                    }

                    // Safe Apply report overlay - now doubles as ship confirmation
                    if let Overlay::SafeApplyReport { branch_name, .. } = &app.overlay {
                        // Check if we're in shipping state
                        if let Some(ship_step) = app.ship_step {
                            match ship_step {
                                ui::ShipStep::Done => {
                                    if key.code == KeyCode::Enter || key.code == KeyCode::Esc {
                                        app.ship_step = None;
                                        app.clear_pending_changes();
                                        app.close_overlay();
                                    }
                                }
                                _ => {
                                    // During shipping, only allow Esc to cancel view
                                    if key.code == KeyCode::Esc {
                                        app.close_overlay();
                                        app.ship_step = None;
                                    }
                                }
                            }
                            continue;
                        }
                        
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                            KeyCode::Down => app.overlay_scroll_down(),
                            KeyCode::Up => app.overlay_scroll_up(),
                            KeyCode::Char('u') => {
                                match app.undo_last_pending_change() {
                                    Ok(()) => {
                                        app.show_toast("Undone (restored backup)");
                                        app.close_overlay();
                                    }
                                    Err(e) => app.show_toast(&e),
                                }
                            }
                            KeyCode::Char('y') => {
                                // Ship inline: stage → commit → push → PR
                                let repo_path = app.repo_path.clone();
                                let branch = branch_name.clone();
                                let commit_message = app.generate_commit_message();
                                let files: Vec<PathBuf> = app.pending_changes.iter()
                                    .map(|c| c.file_path.clone())
                                    .collect();
                                let tx_ship = tx.clone();
                                
                                app.ship_step = Some(ui::ShipStep::Committing);
                                
                                tokio::spawn(async move {
                                    // Stage all files (handle both absolute and relative paths)
                                    for file in &files {
                                        let rel_path = if file.is_absolute() {
                                            file.strip_prefix(&repo_path).ok().map(|p| p.to_path_buf())
                                        } else {
                                            Some(file.clone())
                                        };
                                        
                                        if let Some(path) = rel_path {
                                            if let Err(e) = git_ops::stage_file(&repo_path, path.to_str().unwrap_or_default()) {
                                                let _ = tx_ship.send(BackgroundMessage::ShipError(format!("Stage failed: {}", e)));
                                                return;
                                            }
                                        }
                                    }
                                    
                                    // Validate staging
                                    if let Ok(status) = git_ops::current_status(&repo_path) {
                                        if status.staged.is_empty() {
                                            let _ = tx_ship.send(BackgroundMessage::ShipError("No files staged".to_string()));
                                            return;
                                        }
                                    }
                                    
                                    // Commit
                                    if let Err(e) = git_ops::commit(&repo_path, &commit_message) {
                                        let _ = tx_ship.send(BackgroundMessage::ShipError(format!("Commit failed: {}", e)));
                                        return;
                                    }
                                    let _ = tx_ship.send(BackgroundMessage::ShipProgress(ui::ShipStep::Pushing));
                                    
                                    // Push
                                    if let Err(e) = git_ops::push_branch(&repo_path, &branch) {
                                        let _ = tx_ship.send(BackgroundMessage::ShipError(format!("Push failed: {}", e)));
                                        return;
                                    }
                                    let _ = tx_ship.send(BackgroundMessage::ShipProgress(ui::ShipStep::CreatingPR));
                                    
                                    // Create PR
                                    let pr_title = commit_message.lines().next().unwrap_or("Cosmos fix").to_string();
                                    let pr_body = format!(
                                        "## Summary\n\nAutomated fix applied by Cosmos.\n\n{}\n\n---\n*Created with [Cosmos](https://cosmos.dev)*",
                                        commit_message
                                    );
                                    
                                    match git_ops::create_pr(&repo_path, &pr_title, &pr_body) {
                                        Ok(url) => {
                                            let _ = tx_ship.send(BackgroundMessage::ShipComplete(url));
                                        }
                                        Err(e) => {
                                            let _ = tx_ship.send(BackgroundMessage::ShipError(
                                                format!("Pushed, but PR creation failed: {}. Create PR manually.", e)
                                            ));
                                        }
                                    }
                                });
                            }
                            _ => {}
                        }
                        continue;
                    }

// Handle FixPreview overlay (Phase 1 - fast preview)
                    if let Overlay::FixPreview { suggestion_id, file_path, modifier_input, .. } = &app.overlay {
                        let suggestion_id = *suggestion_id;
                        let file_path = file_path.clone();
                        let modifier_input = modifier_input.clone();
                        let is_typing_modifier = !modifier_input.is_empty();
                        
                        match key.code {
                            KeyCode::Esc => {
                                if is_typing_modifier {
                                    // Clear modifier and stay in preview
                                    if let Overlay::FixPreview { modifier_input, .. } = &mut app.overlay {
                                        modifier_input.clear();
                                    }
                                } else {
                                    app.close_overlay();
                                }
                            }
                            KeyCode::Char('n') if !is_typing_modifier => {
                                app.close_overlay();
                            }
                            KeyCode::Char('y') if !is_typing_modifier => {
                                if let Err(e) = app.config.allow_ai(app.session_cost) {
                                    app.show_toast(&e);
                                    continue;
                                }
                                // Phase 2: Generate and apply fix with Smart preset
                                let preview = if let Overlay::FixPreview { preview, .. } = &app.overlay {
                                    preview.clone()
                                } else {
                                    continue;
                                };
                                
                                // Show warning toast if proceeding with unverified fix
                                if !preview.verified {
                                    app.show_toast("Proceeding with unverified fix...");
                                }
                                
                                if let Some(suggestion) = app.suggestions.suggestions.iter().find(|s| s.id == suggestion_id) {
                                    
                                    let suggestion_clone = suggestion.clone();
                                    let repo_path_clone = repo_path.clone();
                                    let tx_fix = tx.clone();
                                    let file_path_clone = file_path.clone();
                                    
                                    app.loading = LoadingState::GeneratingFix;
                                    app.close_overlay();
                                    
                                    tokio::spawn(async move {
                                        // Create a new branch from main before applying the fix
                                        let branch_name = git_ops::generate_fix_branch_name(
                                            &suggestion_clone.id.to_string(),
                                            &suggestion_clone.summary
                                        );
                                        
                                        // Try to create and checkout the fix branch from main
                                        let created_branch = match git_ops::create_fix_branch_from_main(&repo_path_clone, &branch_name) {
                                            Ok(name) => name,
                                            Err(e) => {
                                                let _ = tx_fix.send(BackgroundMessage::DirectFixError(
                                                    format!("Failed to create fix branch: {}. Please ensure you have no uncommitted changes and 'main' or 'master' branch exists.", e)
                                                ));
                                                return;
                                            }
                                        };
                                        
                                        let full_path = repo_path_clone.join(&file_path_clone);
                                        let content = match std::fs::read_to_string(&full_path) {
                                            Ok(c) => c,
                                            Err(e) => {
                                                let _ = tx_fix.send(BackgroundMessage::DirectFixError(format!("Failed to read file: {}", e)));
                                                return;
                                            }
                                        };
                                        
                                        // Generate the fix content using the human plan (+ repo memory)
                                        let mem = crate::cache::Cache::new(&repo_path_clone)
                                            .load_repo_memory()
                                            .to_prompt_context(12, 900);
                                        let mem = if mem.trim().is_empty() { None } else { Some(mem) };
                                        match suggest::llm::generate_fix_content(&file_path_clone, &content, &suggestion_clone, &preview, mem).await {
                                            Ok(applied_fix) => {
                                                // Create backup before applying
                                                let backup_path = full_path.with_extension("cosmos.bak");
                                                if let Err(e) = std::fs::copy(&full_path, &backup_path) {
                                                    let _ = tx_fix.send(BackgroundMessage::DirectFixError(format!("Failed to create backup: {}", e)));
                                                    return;
                                                }
                                                
                                                // Write the new content
                                                match std::fs::write(&full_path, &applied_fix.new_content) {
                                                    Ok(_) => {
                                                        // Stage immediately after writing - ensures changes are ready to ship
                                                        let rel_path = full_path.strip_prefix(&repo_path_clone)
                                                            .map(|p| p.to_string_lossy().to_string())
                                                            .unwrap_or_else(|_| file_path_clone.to_string_lossy().to_string());
                                                        let _ = git_ops::stage_file(&repo_path_clone, &rel_path);
                                                        
                                                        // Run fast local checks for confidence (best-effort)
                                                        let safety_checks = crate::safe_apply::run(&repo_path_clone);

                                                        let _ = tx_fix.send(BackgroundMessage::DirectFixApplied {
                                                            suggestion_id,
                                                            file_path: file_path_clone,
                                                            description: applied_fix.description,
                                                            modified_areas: applied_fix.modified_areas,
                                                            backup_path,
                                                            safety_checks,
                                                            usage: applied_fix.usage,
                                                            branch_name: created_branch,
                                                        });
                                                    }
                                                    Err(e) => {
                                                        // Restore backup on failure
                                                        let _ = std::fs::copy(&backup_path, &full_path);
                                                        let _ = std::fs::remove_file(&backup_path);
                                                        let _ = tx_fix.send(BackgroundMessage::DirectFixError(format!("Failed to write fix: {}", e)));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                let _ = tx_fix.send(BackgroundMessage::DirectFixError(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                            KeyCode::Char('m') if !is_typing_modifier => {
                                // Start typing modifier - set to single space to trigger typing mode
                                // The space will be trimmed when used, but keeps us in typing mode
                                if let Overlay::FixPreview { modifier_input, .. } = &mut app.overlay {
                                    modifier_input.clear();
                                    modifier_input.push(' ');
                                }
                            }
                            KeyCode::Enter if is_typing_modifier => {
                                // Regenerate preview with modifier
                                if let Some(suggestion) = app.suggestions.suggestions.iter().find(|s| s.id == suggestion_id) {
                                    if let Err(e) = app.config.allow_ai(app.session_cost) {
                                        app.show_toast(&e);
                                        continue;
                                    }
                                    let suggestion_clone = suggestion.clone();
                                    let summary = suggestion.summary.clone();
                                    let file_path_clone = file_path.clone();
                                    let trimmed = modifier_input.trim();
                                    let modifier = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
                                    let tx_preview = tx.clone();
                                    let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                    
                                    app.loading = LoadingState::GeneratingPreview;
                                    app.close_overlay();
                                    
                                    tokio::spawn(async move {
                                        let mem = if repo_memory_context.trim().is_empty() {
                                            None
                                        } else {
                                            Some(repo_memory_context)
                                        };
                                        match suggest::llm::generate_fix_preview(&file_path_clone, &suggestion_clone, modifier.as_deref(), mem).await {
                                            Ok(preview) => {
                                                let _ = tx_preview.send(BackgroundMessage::PreviewReady {
                                                    suggestion_id,
                                                    file_path: file_path_clone,
                                                    summary,
                                                    preview,
                                                });
                                            }
                                            Err(e) => {
                                                let _ = tx_preview.send(BackgroundMessage::PreviewError(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                            KeyCode::Char('d') if !is_typing_modifier => {
                                // Dismiss the suggestion (especially useful when not verified)
                                app.suggestions.dismiss(suggestion_id);
                                app.show_toast("Dismissed");
                                app.close_overlay();
                            }
                            KeyCode::Backspace if is_typing_modifier => {
                                app.preview_modifier_pop();
                            }
                            KeyCode::Char(c) if is_typing_modifier => {
                                app.preview_modifier_push(c);
                            }
                            _ => {}
                        }
                        continue;
                    }
                    
                    // Handle BranchCreate overlay
                    if let Overlay::BranchCreate { branch_name, commit_message, pending_files } = &app.overlay {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                            KeyCode::Char('y') => {
                                // Execute branch creation and commit
                                let repo_path = app.repo_path.clone();
                                let branch = branch_name.clone();
                                let message = commit_message.clone();
                                let files = pending_files.clone();
                                
                                app.close_overlay();
                                app.show_toast("Creating branch...");
                                
                                // Create branch, stage files, commit, and push
                                match git_ops::create_and_checkout_branch(&repo_path, &branch) {
                                    Ok(()) => {
                                        // Stage all pending files
                                        for file in &files {
                                            if let Some(rel_path) = file.strip_prefix(&repo_path).ok().and_then(|p| p.to_str()) {
                                                let _ = git_ops::stage_file(&repo_path, rel_path);
                                            }
                                        }
                                        
                                        // Commit
                                        match git_ops::commit(&repo_path, &message) {
                                            Ok(_) => {
                                                app.cosmos_branch = Some(branch.clone());
                                                
                                                // Try to push (non-blocking)
                                                let repo_for_push = repo_path.clone();
                                                let branch_for_push = branch.clone();
                                                let tx_push = tx.clone();
                                                tokio::spawn(async move {
                                                    match git_ops::push_branch(&repo_for_push, &branch_for_push) {
                                                        Ok(_) => {
                                                            let _ = tx_push.send(BackgroundMessage::Error("Pushed! Press 'p' for PR".to_string()));
                                                        }
                                                        Err(e) => {
                                                            let _ = tx_push.send(BackgroundMessage::Error(format!("Push failed: {}", e)));
                                                        }
                                                    }
                                                });
                                                
                                                app.show_toast("Branch created and committed");
                                            }
                                            Err(e) => {
                                                app.show_toast(&format!("Commit failed: {}", e));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        app.show_toast(&format!("Branch failed: {}", e));
                                    }
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }
                    
                    // Handle ShipDialog overlay - streamlined commit + push + PR flow
                    if let Overlay::ShipDialog { branch_name, commit_message, files, step, .. } = &app.overlay {
                        let step = *step;
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                if step == ui::ShipStep::Confirm || step == ui::ShipStep::Done {
                                    app.close_overlay();
                                }
                                // Don't allow cancel during in-progress steps
                            }
                            KeyCode::Down => app.overlay_scroll_down(),
                            KeyCode::Up => app.overlay_scroll_up(),
                            KeyCode::Char('y') if step == ui::ShipStep::Confirm => {
                                // Execute the full ship workflow: stage → commit → push → PR
                                let repo_path = app.repo_path.clone();
                                let branch = branch_name.clone();
                                let message = commit_message.clone();
                                let files = files.clone();
                                let tx_ship = tx.clone();
                                
                                app.update_ship_step(ui::ShipStep::Committing);
                                
                                tokio::spawn(async move {
                                    // Step 1: Stage all files (handle both absolute and relative paths)
                                    for file in &files {
                                        let rel_path = if file.is_absolute() {
                                            file.strip_prefix(&repo_path).ok().map(|p| p.to_path_buf())
                                        } else {
                                            Some(file.clone())
                                        };
                                        
                                        if let Some(path) = rel_path {
                                            if let Err(e) = git_ops::stage_file(&repo_path, path.to_str().unwrap_or_default()) {
                                                let _ = tx_ship.send(BackgroundMessage::ShipError(format!("Stage failed: {}", e)));
                                                return;
                                            }
                                        }
                                    }
                                    
                                    // Validate: ensure something is staged before committing
                                    if let Ok(status) = git_ops::current_status(&repo_path) {
                                        if status.staged.is_empty() {
                                            let _ = tx_ship.send(BackgroundMessage::ShipError("No files staged - nothing to commit".to_string()));
                                            return;
                                        }
                                    }
                                    
                                    // Step 2: Commit
                                    if let Err(e) = git_ops::commit(&repo_path, &message) {
                                        let _ = tx_ship.send(BackgroundMessage::ShipError(format!("Commit failed: {}", e)));
                                        return;
                                    }
                                    let _ = tx_ship.send(BackgroundMessage::ShipProgress(ui::ShipStep::Pushing));
                                    
                                    // Step 3: Push
                                    if let Err(e) = git_ops::push_branch(&repo_path, &branch) {
                                        let _ = tx_ship.send(BackgroundMessage::ShipError(format!("Push failed: {}", e)));
                                        return;
                                    }
                                    let _ = tx_ship.send(BackgroundMessage::ShipProgress(ui::ShipStep::CreatingPR));
                                    
                                    // Step 4: Create PR
                                    let pr_title = message.lines().next().unwrap_or("Cosmos fix").to_string();
                                    let pr_body = format!(
                                        "## Summary\n\nAutomated fix applied by Cosmos.\n\n{}\n\n---\n*Created with [Cosmos](https://cosmos.dev)*",
                                        message
                                    );
                                    
                                    match git_ops::create_pr(&repo_path, &pr_title, &pr_body) {
                                        Ok(url) => {
                                            let _ = tx_ship.send(BackgroundMessage::ShipComplete(url));
                                        }
                                        Err(e) => {
                                            // PR creation failed but commit/push succeeded
                                            let _ = tx_ship.send(BackgroundMessage::ShipError(
                                                format!("Pushed, but PR creation failed: {}. Create PR manually.", e)
                                            ));
                                        }
                                    }
                                });
                            }
                            KeyCode::Enter if step == ui::ShipStep::Done => {
                                // Open the PR URL in browser
                                if let Some(url) = &app.pr_url {
                                    let _ = git_ops::open_url(url);
                                }
                                app.clear_pending_changes();
                                app.close_overlay();
                            }
                            _ => {}
                        }
                        continue;
                    }
                    
                    // Handle VerificationReview overlay (adversarial code review)
                    if let Overlay::VerificationReview {
                        file_path, original_content, new_content, findings, selected, reviewing, fixing, review_iteration, fixed_titles, ..
                    } = &app.overlay {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                if !*reviewing && !*fixing {
                                    app.close_overlay();
                                }
                            }
                            KeyCode::Down => {
                                // Move cursor down
                                if !*reviewing && !*fixing {
                                    app.verification_cursor_down();
                                }
                            }
                            KeyCode::Up => {
                                // Move cursor up
                                if !*reviewing && !*fixing {
                                    app.verification_cursor_up();
                                }
                            }
                            KeyCode::Char(' ') | KeyCode::Char('x') => {
                                // Toggle selection of finding at cursor
                                if !*reviewing && !*fixing && !findings.is_empty() {
                                    app.toggle_finding_at_cursor();
                                }
                            }
                            KeyCode::Char('a') => {
                                // Select all
                                if !*reviewing && !*fixing {
                                    app.select_all_findings();
                                }
                            }
                            KeyCode::Char('n') => {
                                // Select none
                                if !*reviewing && !*fixing {
                                    app.deselect_all_findings();
                                }
                            }
                            KeyCode::Char('f') => {
                                // Fix selected findings
                                if !*reviewing && !*fixing && !selected.is_empty() {
                                    let selected_findings = app.get_selected_findings();
                                    let file = file_path.clone();
                                    let content = new_content.clone();
                                    let original = original_content.clone();
                                    let iter = *review_iteration;
                                    let fixed = fixed_titles.clone();
                                    let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                    let memory = if repo_memory_context.trim().is_empty() { None } else { Some(repo_memory_context) };
                                    let tx_fix = tx.clone();

                                    app.set_verification_fixing(true);

                                    tokio::spawn(async move {
                                        // Pass original content for context on later iterations
                                        let orig_ref = if iter > 1 { Some(original.as_str()) } else { None };
                                        match suggest::llm::fix_review_findings(
                                            &file, 
                                            &content,
                                            orig_ref,
                                            &selected_findings,
                                            memory,
                                            iter,
                                            &fixed,
                                        ).await {
                                            Ok(fix) => {
                                                let _ = tx_fix.send(BackgroundMessage::VerificationFixComplete {
                                                    new_content: fix.new_content,
                                                    description: fix.description,
                                                    usage: fix.usage,
                                                });
                                            }
                                            Err(e) => {
                                                let _ = tx_fix.send(BackgroundMessage::Error(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                            KeyCode::Char('r') => {
                                // Re-run verification review
                                if !*reviewing && !*fixing {
                                    let file = file_path.clone();
                                    let orig = original_content.clone();
                                    let new = new_content.clone();
                                    let iter = *review_iteration;
                                    let fixed = fixed_titles.clone();
                                    let tx_verify = tx.clone();

                                    // Reset to reviewing state
                                    if let Overlay::VerificationReview { reviewing, findings, selected, summary, .. } = &mut app.overlay {
                                        *reviewing = true;
                                        findings.clear();
                                        selected.clear();
                                        *summary = String::new();
                                    }
                                    app.loading = LoadingState::ReviewingChanges;

                                    tokio::spawn(async move {
                                        let files_with_content = vec![(file, orig, new)];
                                        match suggest::llm::verify_changes(&files_with_content, iter, &fixed).await {
                                            Ok(review) => {
                                                let _ = tx_verify.send(BackgroundMessage::VerificationComplete {
                                                    findings: review.findings,
                                                    summary: review.summary,
                                                    usage: review.usage,
                                                });
                                            }
                                            Err(e) => {
                                                let _ = tx_verify.send(BackgroundMessage::Error(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                            KeyCode::Char('s') | KeyCode::Enter => {
                                // Skip remaining issues and accept / ship
                                if !*reviewing && !*fixing {
                                    // Close overlay and accept the changes
                                    app.close_overlay();
                                    app.show_toast("Changes accepted");
                                }
                            }
                            KeyCode::PageDown => {
                                // Page down through findings
                                if !*reviewing && !*fixing {
                                    app.verification_page_down();
                                }
                            }
                            KeyCode::PageUp => {
                                // Page up through findings
                                if !*reviewing && !*fixing {
                                    app.verification_page_up();
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle PRReview overlay
                    if let Overlay::PRReview { files_changed, reviewing, pr_url, .. } = &app.overlay {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                            KeyCode::Down => app.overlay_scroll_down(),
                            KeyCode::Up => app.overlay_scroll_up(),
                            KeyCode::Char('r') => {
                                if !*reviewing {
                                    // Start AI review
                                    let files = files_changed.clone();
                                    let tx_review = tx.clone();
                                    
                                    // Set reviewing state
                                    if let Overlay::PRReview { reviewing, .. } = &mut app.overlay {
                                        *reviewing = true;
                                    }
                                    app.loading = LoadingState::ReviewingChanges;
                                    
                                    tokio::spawn(async move {
                                        match suggest::llm::review_changes(&files).await {
                                            Ok((comments, _usage)) => {
                                                let _ = tx_review.send(BackgroundMessage::ReviewReady(comments));
                                            }
                                            Err(e) => {
                                                let _ = tx_review.send(BackgroundMessage::Error(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                            KeyCode::Char('c') => {
                                // Create PR
                                if pr_url.is_none() {
                                    let repo_path = app.repo_path.clone();
                                    let commit_message = app.generate_commit_message();
                                    let tx_pr = tx.clone();
                                    
                                    app.show_toast("Creating PR...");
                                    
                                    tokio::spawn(async move {
                                        let pr_title = commit_message.lines().next()
                                            .unwrap_or("Cosmos fix").to_string();
                                        let pr_body = format!(
                                            "## Summary\n\n{}\n\n---\n*Applied with Cosmos*",
                                            commit_message
                                        );
                                        match git_ops::create_pr(&repo_path, &pr_title, &pr_body) {
                                            Ok(url) => {
                                                let _ = tx_pr.send(BackgroundMessage::PRCreated(url));
                                            }
                                            Err(e) => {
                                                let _ = tx_pr.send(BackgroundMessage::Error(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                            KeyCode::Char('o') => {
                                // Open PR in browser
                                if let Some(url) = pr_url {
                                    let _ = git_ops::open_url(url);
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }
                    
                    // Handle GitStatus overlay - simplified interface
                    if let Overlay::GitStatus { commit_input, .. } = &app.overlay {
                        // Check if we're in commit input mode
                        if commit_input.is_some() {
                            match key.code {
                                KeyCode::Esc => {
                                    app.git_cancel_commit();
                                }
                                KeyCode::Enter => {
                                    match app.git_do_commit() {
                                        Ok(_oid) => {
                                            app.show_toast("+ Committed - Press 's' to Ship");
                                            app.close_overlay();
                                        }
                                        Err(e) => {
                                            app.show_toast(&e);
                                        }
                                    }
                                }
                                KeyCode::Backspace => {
                                    app.git_commit_pop();
                                }
                                KeyCode::Char(c) => {
                                    app.git_commit_push(c);
                                }
                                _ => {}
                            }
                        } else {
                            // Clean, simple navigation and actions
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('q') => {
                                    app.close_overlay();
                                }
                                KeyCode::Down => {
                                    app.git_status_navigate(1);
                                }
                                KeyCode::Up => {
                                    app.git_status_navigate(-1);
                                }
                                KeyCode::Char('s') | KeyCode::Enter => {
                                    // Stage selected file (Enter as alias for convenience)
                                    app.git_stage_selected();
                                }
                                KeyCode::Char('u') => {
                                    // Unstage selected file
                                    app.git_unstage_selected();
                                }
                                KeyCode::Char('c') => {
                                    // Start commit
                                    app.git_start_commit();
                                }
                                _ => {}
                            }
                        }
                        continue;
                    }
                    
                    // Handle ErrorLog overlay
                    if let Overlay::ErrorLog { .. } = &app.overlay {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('e') => app.close_overlay(),
                            KeyCode::Down => {
                                // Navigate down in error log
                                let max = app.error_log.len().saturating_sub(1);
                                if let Overlay::ErrorLog { selected, scroll } = &mut app.overlay {
                                    if *selected < max {
                                        *selected += 1;
                                    }
                                    // Keep selected in view
                                    let visible = 10;
                                    if *selected >= *scroll + visible {
                                        *scroll = selected.saturating_sub(visible - 1);
                                    }
                                }
                            }
                            KeyCode::Up => {
                                // Navigate up in error log
                                if let Overlay::ErrorLog { selected, scroll } = &mut app.overlay {
                                    *selected = selected.saturating_sub(1);
                                    if *selected < *scroll {
                                        *scroll = *selected;
                                    }
                                }
                            }
                            KeyCode::Char('c') => {
                                // Clear error log
                                app.clear_error_log();
                                app.close_overlay();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle Reset cosmos overlay
                    if let ui::Overlay::Reset { .. } = &app.overlay {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                app.close_overlay();
                            }
                            KeyCode::Down => {
                                app.reset_navigate(1);
                            }
                            KeyCode::Up => {
                                app.reset_navigate(-1);
                            }
                            KeyCode::Char(' ') => {
                                app.reset_toggle_selected();
                            }
                            KeyCode::Enter => {
                                // Execute reset with selected options
                                let selections = app.get_reset_selections();
                                if selections.is_empty() {
                                    app.show_toast("No options selected");
                                } else {
                                    // Clear selected caches
                                    let cache = crate::cache::Cache::new(&app.repo_path);
                                    match cache.clear_selective(&selections) {
                                        Ok(cleared) => {
                                            app.close_overlay();
                                            
                                            // Check if we need to regenerate things
                                            let needs_reindex = selections.contains(&crate::cache::ResetOption::Index);
                                            let needs_suggestions = selections.contains(&crate::cache::ResetOption::Suggestions);
                                            let needs_summaries = selections.contains(&crate::cache::ResetOption::Summaries);
                                            let needs_glossary = selections.contains(&crate::cache::ResetOption::Glossary);
                                            
                                            // Perform reindex if needed
                                            if needs_reindex {
                                                match index::CodebaseIndex::new(&app.repo_path) {
                                                    Ok(new_index) => {
                                                        // Apply grouping
                                                        let mut idx = new_index;
                                                        let grouping = idx.generate_grouping();
                                                        idx.apply_grouping(&grouping);
                                                        app.index = idx;
                                                        app.grouping = grouping;
                                                    }
                                                    Err(e) => {
                                                        app.show_toast(&format!("Reindex failed: {}", e));
                                                    }
                                                }
                                            }
                                            
                                            // Clear in-memory suggestions if needed
                                            if needs_suggestions {
                                                app.suggestions = suggest::SuggestionEngine::new_empty(app.index.clone());
                                            }
                                            
                                            // Clear in-memory summaries if needed
                                            if needs_summaries {
                                                app.llm_summaries.clear();
                                                app.needs_summary_generation = true;
                                                app.summary_progress = None;
                                            }
                                            
                                            // Clear in-memory glossary if needed
                                            if needs_glossary {
                                                app.glossary = crate::cache::DomainGlossary::default();
                                            }
                                            
                                            // Refresh context
                                            let _ = app.context.refresh();
                                            
                                            // Check if AI is available for regeneration
                                            let ai_enabled = suggest::llm::is_available() && app.config.allow_ai(app.session_cost).is_ok();
                                            
                                            // IMPORTANT: Summaries must generate FIRST (they build the glossary),
                                            // THEN suggestions can use the rebuilt glossary.
                                            // We track pending_suggestions_on_init to trigger suggestions after summaries complete.
                                            
                                            // Trigger regeneration of summaries first (builds glossary)
                                            if needs_summaries && ai_enabled {
                                                let index_clone2 = app.index.clone();
                                                let context_clone2 = app.context.clone();
                                                let tx_summaries = tx.clone();
                                                let cache_path = repo_path.clone();
                                                
                                                // Compute file hashes for change detection
                                                let file_hashes = cache::compute_file_hashes(&index_clone2);
                                                let file_hashes_clone = file_hashes.clone();
                                                
                                                // All files need summaries after reset
                                                let files_needing_summary: Vec<PathBuf> = file_hashes.keys().cloned().collect();
                                                
                                                // Discover project context
                                                let project_context = suggest::llm::discover_project_context(&index_clone2);
                                                
                                                // Prioritize files for generation
                                                let (high_priority, medium_priority, low_priority) = 
                                                    suggest::llm::prioritize_files_for_summary(&index_clone2, &context_clone2, &files_needing_summary);
                                                
                                                let total_to_process = high_priority.len() + medium_priority.len() + low_priority.len();
                                                
                                                if total_to_process > 0 {
                                                    app.loading = LoadingState::GeneratingSummaries;
                                                    app.summary_progress = Some((0, total_to_process));
                                                    
                                                    // Flag that suggestions should generate after summaries complete
                                                    if needs_suggestions {
                                                        app.pending_suggestions_on_init = true;
                                                    }
                                                    
                                                    tokio::spawn(async move {
                                                        let cache = cache::Cache::new(&cache_path);
                                                        
                                                        // Start with fresh cache after reset
                                                        let mut llm_cache = cache::LlmSummaryCache::new();
                                                        let mut glossary = cache::DomainGlossary::new();
                                                        
                                                        let mut all_summaries = HashMap::new();
                                                        let mut total_usage = suggest::llm::Usage::default();
                                                        let mut completed_count = 0usize;
                                                        
                                                        let priority_tiers = [
                                                            ("high", high_priority),
                                                            ("medium", medium_priority), 
                                                            ("low", low_priority),
                                                        ];
                                                        
                                                        for (_tier_name, files) in priority_tiers {
                                                            if files.is_empty() {
                                                                continue;
                                                            }
                                                            
                                                            let batch_size = 16;
                                                            let batches: Vec<_> = files.chunks(batch_size).collect();
                                                            
                                                            for batch in batches {
                                                                if let Ok((summaries, batch_glossary, usage)) = suggest::llm::generate_summaries_for_files(
                                                                    &index_clone2, batch, &project_context
                                                                ).await {
                                                                    for (path, summary) in &summaries {
                                                                        if let Some(hash) = file_hashes_clone.get(path) {
                                                                            llm_cache.set_summary(path.clone(), summary.clone(), hash.clone());
                                                                        }
                                                                    }
                                                                    glossary.merge(&batch_glossary);
                                                                    
                                                                    let _ = cache.save_llm_summaries_cache(&llm_cache);
                                                                    let _ = cache.save_glossary(&glossary);
                                                                    
                                                                    completed_count += summaries.len();
                                                                    
                                                                    let _ = tx_summaries.send(BackgroundMessage::SummaryProgress {
                                                                        completed: completed_count,
                                                                        total: total_to_process,
                                                                        summaries: summaries.clone(),
                                                                    });
                                                                    
                                                                    all_summaries.extend(summaries);
                                                                    if let Some(u) = usage {
                                                                        total_usage.prompt_tokens += u.prompt_tokens;
                                                                        total_usage.completion_tokens += u.completion_tokens;
                                                                        total_usage.total_tokens += u.total_tokens;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        
                                                        let final_usage = if total_usage.total_tokens > 0 {
                                                            Some(total_usage)
                                                        } else {
                                                            None
                                                        };
                                                        
                                                        let _ = tx_summaries.send(BackgroundMessage::SummariesReady { 
                                                            summaries: HashMap::new(), 
                                                            usage: final_usage 
                                                        });
                                                    });
                                                }
                                            } else if needs_suggestions && ai_enabled {
                                                // No summaries to generate, so generate suggestions directly
                                                let index_clone = app.index.clone();
                                                let context_clone = app.context.clone();
                                                let tx_suggestions = tx.clone();
                                                let cache_clone_path = repo_path.clone();
                                                let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                                let glossary_clone = app.glossary.clone();
                                                
                                                app.loading = LoadingState::GeneratingSuggestions;
                                                
                                                tokio::spawn(async move {
                                                    let mem = if repo_memory_context.trim().is_empty() {
                                                        None
                                                    } else {
                                                        Some(repo_memory_context)
                                                    };
                                                    let glossary_ref = if glossary_clone.is_empty() { None } else { Some(&glossary_clone) };
                                                    match suggest::llm::analyze_codebase(&index_clone, &context_clone, mem, glossary_ref).await {
                                                        Ok((suggestions, usage)) => {
                                                            // Cache the suggestions
                                                            let cache = cache::Cache::new(&cache_clone_path);
                                                            let cache_data = cache::SuggestionsCache::from_suggestions(&suggestions);
                                                            let _ = cache.save_suggestions_cache(&cache_data);
                                                            
                                                            let _ = tx_suggestions.send(BackgroundMessage::SuggestionsReady {
                                                                suggestions,
                                                                usage,
                                                                model: "smart".to_string(),
                                                            });
                                                        }
                                                        Err(e) => {
                                                            let _ = tx_suggestions.send(BackgroundMessage::SuggestionsError(e.to_string()));
                                                        }
                                                    }
                                                });
                                            }
                                            
                                            // Show what was cleared
                                            let count = cleared.len();
                                            if count > 0 {
                                                if !needs_suggestions && !needs_summaries {
                                                    app.show_toast(&format!("Reset complete: {} files cleared", count));
                                                }
                                                // If regenerating, toast was already shown above
                                            } else {
                                                app.show_toast("Reset complete (caches were already empty)");
                                            }
                                        }
                                        Err(e) => {
                                            app.show_toast(&format!("Reset failed: {}", e));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }
                    
                    // Handle other overlays
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                        KeyCode::Down => app.overlay_scroll_down(),
                        KeyCode::Up => app.overlay_scroll_up(),
                        KeyCode::Char('a') => {
                            if let Overlay::SuggestionDetail { suggestion_id, .. } = &app.overlay {
                                let suggestion_id = *suggestion_id;
                                if let Some(suggestion) = app.suggestions.suggestions.iter().find(|s| s.id == suggestion_id) {
                                    if !suggest::llm::is_available() {
                                        app.show_toast("Run: cosmos --setup");
                                    } else {
                                        if let Err(e) = app.config.allow_ai(app.session_cost) {
                                            app.show_toast(&e);
                                            continue;
                                        }
                                        // Phase 1: Generate quick preview (fast)
                                        let file_path = suggestion.file.clone();
                                        let summary = suggestion.summary.clone();
                                        let suggestion_clone = suggestion.clone();
                                        let tx_preview = tx.clone();
                                        let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                        
                                        app.loading = LoadingState::GeneratingPreview;
                                        app.close_overlay();
                                        
                                        tokio::spawn(async move {
                                            let mem = if repo_memory_context.trim().is_empty() {
                                                None
                                            } else {
                                                Some(repo_memory_context)
                                            };
                                            match suggest::llm::generate_fix_preview(&file_path, &suggestion_clone, None, mem).await {
                                                Ok(preview) => {
                                                    let _ = tx_preview.send(BackgroundMessage::PreviewReady {
                                                        suggestion_id,
                                                        file_path,
                                                        summary,
                                                        preview,
                                                    });
                                                }
                                                Err(e) => {
                                                    let _ = tx_preview.send(BackgroundMessage::PreviewError(e.to_string()));
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                        }
                        KeyCode::Char('d') => {
                            if let Overlay::SuggestionDetail { suggestion_id, .. } = &app.overlay {
                                let id = *suggestion_id;
                                app.suggestions.dismiss(id);
                                app.show_toast("Dismissed");
                                app.close_overlay();
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Normal mode
                match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Tab => app.toggle_panel(),
                    KeyCode::Down => {
                        // Handle navigation based on workflow step
                        if app.active_panel == ActivePanel::Suggestions {
                            match app.workflow_step {
                                WorkflowStep::Review if !app.review_state.reviewing && !app.review_state.fixing => {
                                    app.review_cursor_down();
                                }
                                WorkflowStep::Verify if !app.verify_state.loading => {
                                    app.verify_scroll_down();
                                }
                                WorkflowStep::Ship => {
                                    app.ship_scroll_down();
                                }
                                WorkflowStep::Suggestions => app.navigate_down(),
                                _ => {}
                            }
                        } else {
                            app.navigate_down();
                        }
                    }
                    KeyCode::Up => {
                        // Handle navigation based on workflow step
                        if app.active_panel == ActivePanel::Suggestions {
                            match app.workflow_step {
                                WorkflowStep::Review if !app.review_state.reviewing && !app.review_state.fixing => {
                                    app.review_cursor_up();
                                }
                                WorkflowStep::Verify if !app.verify_state.loading => {
                                    app.verify_scroll_up();
                                }
                                WorkflowStep::Ship => {
                                    app.ship_scroll_up();
                                }
                                WorkflowStep::Suggestions => app.navigate_up(),
                                _ => {}
                            }
                        } else {
                            app.navigate_up();
                        }
                    }
                    KeyCode::Char(' ') => {
                        // Space toggles finding selection in Review step
                        if app.active_panel == ActivePanel::Suggestions && app.workflow_step == WorkflowStep::Review {
                            if !app.review_state.reviewing && !app.review_state.fixing {
                                app.review_toggle_finding();
                            }
                        }
                    }
                    KeyCode::Char('f') => {
                        // Fix selected findings in Review step
                        if app.active_panel == ActivePanel::Suggestions 
                           && app.workflow_step == WorkflowStep::Review
                           && !app.review_state.reviewing
                           && !app.review_state.fixing
                           && !app.review_state.selected.is_empty() {
                            let selected_findings = app.get_selected_review_findings();
                            let file = app.review_state.file_path.clone();
                            let content = app.review_state.new_content.clone();
                            let original = app.review_state.original_content.clone();
                            let iter = app.review_state.review_iteration;
                            let fixed = app.review_state.fixed_titles.clone();
                            let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                            let memory = if repo_memory_context.trim().is_empty() { None } else { Some(repo_memory_context) };
                            let tx_fix = tx.clone();

                            if let Some(file_path) = file {
                                app.set_review_fixing(true);

                                tokio::spawn(async move {
                                    let orig_ref = if iter > 1 { Some(original.as_str()) } else { None };
                                    match suggest::llm::fix_review_findings(
                                        &file_path, 
                                        &content,
                                        orig_ref,
                                        &selected_findings,
                                        memory,
                                        iter,
                                        &fixed,
                                    ).await {
                                        Ok(fix) => {
                                            let _ = tx_fix.send(BackgroundMessage::VerificationFixComplete {
                                                new_content: fix.new_content,
                                                description: fix.description,
                                                usage: fix.usage,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = tx_fix.send(BackgroundMessage::Error(e.to_string()));
                                        }
                                    }
                                });
                            }
                        }
                    }
                    KeyCode::Enter => {
                        // If PR URL is pending, open it in browser
                        if let Some(url) = app.pr_url.take() {
                            let _ = git_ops::open_url(&url);
                        } else {
                            match app.active_panel {
                                ActivePanel::Project => app.toggle_group_expand(),
                                ActivePanel::Suggestions => {
                                    // Handle based on workflow step
                                    match app.workflow_step {
                                        WorkflowStep::Suggestions => {
                                            // Start verify step with selected suggestion
                                            let suggestion = app.selected_suggestion().cloned();
                                            if let Some(suggestion) = suggestion {
                                                if !suggest::llm::is_available() {
                                                    app.show_toast("Run: cosmos --setup");
                                                } else {
                                                    if let Err(e) = app.config.allow_ai(app.session_cost) {
                                                        app.show_toast(&e);
                                                        continue;
                                                    }
                                                    let suggestion_id = suggestion.id;
                                                    let file_path = suggestion.file.clone();
                                                    let summary = suggestion.summary.clone();
                                                    let suggestion_clone = suggestion.clone();
                                                    let tx_preview = tx.clone();
                                                    let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                                    
                                                    // Move to Verify step
                                                    app.start_verify(suggestion_id, file_path.clone(), summary.clone());
                                                    
                                                    tokio::spawn(async move {
                                                        let mem = if repo_memory_context.trim().is_empty() {
                                                            None
                                                        } else {
                                                            Some(repo_memory_context)
                                                        };
                                                        match suggest::llm::generate_fix_preview(&file_path, &suggestion_clone, None, mem).await {
                                                            Ok(preview) => {
                                                                let _ = tx_preview.send(BackgroundMessage::PreviewReady {
                                                                    suggestion_id,
                                                                    file_path,
                                                                    summary,
                                                                    preview,
                                                                });
                                                            }
                                                            Err(e) => {
                                                                let _ = tx_preview.send(BackgroundMessage::PreviewError(e.to_string()));
                                                            }
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                        WorkflowStep::Verify => {
                                            // Apply the fix and move to Review
                                            if let Some(preview) = app.verify_state.preview.clone() {
                                                let state = &app.verify_state;
                                                let suggestion_id = state.suggestion_id;
                                                let file_path = state.file_path.clone();
                                                let tx_apply = tx.clone();
                                                let repo_path = app.repo_path.clone();
                                                let repo_memory_context = app.repo_memory.to_prompt_context(12, 900);
                                                
                                                if let (Some(sid), Some(fp)) = (suggestion_id, file_path.clone()) {
                                                    if let Some(suggestion) = app.suggestions.suggestions.iter().find(|s| s.id == sid).cloned() {
                                                        app.loading = LoadingState::GeneratingFix;
                                                        
                                                        tokio::spawn(async move {
                                                            // Create branch from main
                                                            let branch_name = git_ops::generate_fix_branch_name(
                                                                &suggestion.id.to_string(),
                                                                &suggestion.summary
                                                            );
                                                            
                                                            let created_branch = match git_ops::create_fix_branch_from_main(&repo_path, &branch_name) {
                                                                Ok(name) => name,
                                                                Err(e) => {
                                                                    let _ = tx_apply.send(BackgroundMessage::DirectFixError(
                                                                        format!("Failed to create fix branch: {}", e)
                                                                    ));
                                                                    return;
                                                                }
                                                            };
                                                            
                                                            let full_path = repo_path.join(&fp);
                                                            let content = match std::fs::read_to_string(&full_path) {
                                                                Ok(c) => c,
                                                                Err(e) => {
                                                                    let _ = tx_apply.send(BackgroundMessage::DirectFixError(format!("Failed to read file: {}", e)));
                                                                    return;
                                                                }
                                                            };
                                                            
                                                            let mem = if repo_memory_context.trim().is_empty() { None } else { Some(repo_memory_context) };
                                                            match suggest::llm::generate_fix_content(&fp, &content, &suggestion, &preview, mem).await {
                                                                Ok(applied_fix) => {
                                                                    let backup_path = full_path.with_extension("cosmos.bak");
                                                                    if let Err(e) = std::fs::copy(&full_path, &backup_path) {
                                                                        let _ = tx_apply.send(BackgroundMessage::DirectFixError(format!("Failed to create backup: {}", e)));
                                                                        return;
                                                                    }
                                                                    
                                                                    match std::fs::write(&full_path, &applied_fix.new_content) {
                                                                        Ok(_) => {
                                                                            let rel_path = full_path.strip_prefix(&repo_path)
                                                                                .map(|p| p.to_string_lossy().to_string())
                                                                                .unwrap_or_else(|_| fp.to_string_lossy().to_string());
                                                                            let _ = git_ops::stage_file(&repo_path, &rel_path);
                                                                            
                                                                            let safety_checks = crate::safe_apply::run(&repo_path);
                                                                            
                                                                            let _ = tx_apply.send(BackgroundMessage::DirectFixApplied {
                                                                                suggestion_id: sid,
                                                                                file_path: fp,
                                                                                description: applied_fix.description,
                                                                                modified_areas: applied_fix.modified_areas,
                                                                                backup_path,
                                                                                safety_checks,
                                                                                usage: applied_fix.usage,
                                                                                branch_name: created_branch,
                                                                            });
                                                                        }
                                                                        Err(e) => {
                                                                            let _ = std::fs::copy(&backup_path, &full_path);
                                                                            let _ = std::fs::remove_file(&backup_path);
                                                                            let _ = tx_apply.send(BackgroundMessage::DirectFixError(format!("Failed to write fix: {}", e)));
                                                                        }
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    let _ = tx_apply.send(BackgroundMessage::DirectFixError(e.to_string()));
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                        WorkflowStep::Review => {
                                            // If review passed, move to Ship
                                            if app.review_passed() {
                                                app.start_ship();
                                            }
                                        }
                                        WorkflowStep::Ship => {
                                            // Execute ship based on current step
                                            match app.ship_state.step {
                                                ui::ShipStep::Confirm => {
                                                    // Start the ship process
                                                    let repo_path = app.repo_path.clone();
                                                    let branch_name = app.ship_state.branch_name.clone();
                                                    let commit_message = app.ship_state.commit_message.clone();
                                                    let tx_ship = tx.clone();
                                                    
                                                    app.set_ship_step(ui::ShipStep::Committing);
                                                    
                                                    tokio::spawn(async move {
                                                        // Execute ship workflow
                                                        let _ = tx_ship.send(BackgroundMessage::ShipProgress(ui::ShipStep::Committing));
                                                        
                                                        // Commit (files are already staged)
                                                        if let Err(e) = git_ops::commit(&repo_path, &commit_message) {
                                                            let _ = tx_ship.send(BackgroundMessage::ShipError(e.to_string()));
                                                            return;
                                                        }
                                                        
                                                        let _ = tx_ship.send(BackgroundMessage::ShipProgress(ui::ShipStep::Pushing));
                                                        
                                                        // Push
                                                        if let Err(e) = git_ops::push_branch(&repo_path, &branch_name) {
                                                            let _ = tx_ship.send(BackgroundMessage::ShipError(e.to_string()));
                                                            return;
                                                        }
                                                        
                                                        let _ = tx_ship.send(BackgroundMessage::ShipProgress(ui::ShipStep::CreatingPR));
                                                        
                                                        // Create PR with descriptive body from commit message
                                                        let pr_title = commit_message.lines().next()
                                                            .unwrap_or("Cosmos fix").to_string();
                                                        let pr_body = format!(
                                                            "## Summary\n\n{}\n\n---\n*Applied with Cosmos*",
                                                            commit_message
                                                        );
                                                        match git_ops::create_pr(&repo_path, &pr_title, &pr_body) {
                                                            Ok(url) => {
                                                                let _ = tx_ship.send(BackgroundMessage::ShipComplete(url));
                                                            }
                                                            Err(e) => {
                                                                let _ = tx_ship.send(BackgroundMessage::ShipError(e.to_string()));
                                                            }
                                                        }
                                                    });
                                                }
                                                ui::ShipStep::Done => {
                                                    // Open PR in browser and complete workflow
                                                    if let Some(url) = &app.ship_state.pr_url {
                                                        let _ = git_ops::open_url(url);
                                                    }
                                                    app.workflow_complete();
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        // Handle workflow back navigation
                        if app.active_panel == ActivePanel::Suggestions && app.workflow_step != WorkflowStep::Suggestions {
                            app.workflow_back();
                        } else if !app.search_query.is_empty() {
                            app.exit_search();
                        } else if app.overlay != Overlay::None {
                            app.close_overlay();
                        }
                    }
                    KeyCode::Char('/') => app.start_search(),
                    KeyCode::Char('g') => app.toggle_view_mode(),
                    KeyCode::PageDown => app.page_down(),
                    KeyCode::PageUp => app.page_up(),
                    KeyCode::Char('?') => app.toggle_help(),
                    KeyCode::Char('d') => app.dismiss_selected(),
                    KeyCode::Char('a') => {
                        // Handle 'a' based on context
                        if app.active_panel == ActivePanel::Suggestions && app.workflow_step == WorkflowStep::Review {
                            // Select all findings
                            if !app.review_state.reviewing && !app.review_state.fixing {
                                app.review_select_all();
                            }
                        } else {
                            // Legacy: show suggestion detail (now use Enter to verify)
                            app.show_suggestion_detail();
                        }
                    }
                    KeyCode::Char('i') => {
                        if !suggest::llm::is_available() {
                            app.show_toast("Run: cosmos --setup");
                        } else {
                            app.start_question();
                        }
                    }
                    KeyCode::Char('b') => {
                        // Branch workflow - create branch and commit pending changes
                        // If already on a cosmos branch, this shows ship dialog
                        app.show_branch_dialog();
                    }
                    KeyCode::Char('s') => {
                        // Handle 's' based on context
                        if app.active_panel == ActivePanel::Suggestions && app.workflow_step == WorkflowStep::Review {
                            // Skip to Ship (skip remaining review findings)
                            if !app.review_state.reviewing && !app.review_state.fixing {
                                app.start_ship();
                            }
                        } else {
                            // Legacy ship workflow
                            if app.is_ready_to_ship() {
                                app.show_ship_dialog();
                            } else if app.pending_change_count() > 0 {
                                app.show_toast("First apply a fix (creates branch), then ship");
                            } else {
                                app.show_toast("No changes to ship");
                            }
                        }
                    }
                    KeyCode::Char('u') => {
                        // Undo the last applied change (restore backup)
                        match app.undo_last_pending_change() {
                            Ok(()) => app.show_toast("Undone (restored backup)"),
                            Err(e) => app.show_toast(&e),
                        }
                    }
                    KeyCode::Char('c') => {
                        // Git status - view and manage changed files
                        app.show_git_status();
                    }
                    KeyCode::Char('m') => {
                        // Switch to main branch (quick escape from fix branches)
                        if app.is_on_main_branch() {
                            app.show_toast("Already on main");
                        } else {
                            match app.git_switch_to_main() {
                                Ok(_) => {
                                    app.show_toast("Switched to main");
                                    let _ = app.context.refresh();
                                }
                                Err(e) => app.show_toast(&e),
                            }
                        }
                    }
                    KeyCode::Char('M') => {
                        // Repo memory (decisions/conventions)
                        app.show_repo_memory();
                    }
                    KeyCode::Char('R') => {
                        // Open reset cosmos overlay
                        app.open_reset_overlay();
                    }
                    KeyCode::Char('e') => {
                        // Show error log overlay
                        app.show_error_log();
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

