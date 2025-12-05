//! Cosmos - A contemplative vibe coding companion
//!
//! ☽ C O S M O S ✦
//!
//! An AI-powered IDE in the terminal that uses codebase indexing
//! to suggest improvements, bug fixes, and optimizations.

mod cache;
mod config;
mod context;
mod index;
mod suggest;
mod ui;

// Keep these for compatibility during transition
mod ai;
mod analysis;
mod diff;
mod git_ops;
mod history;
mod mascot;
mod prompt;
mod refactor;
mod score;
mod spinner;
mod testing;
mod workflow;

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
use std::io;
use std::path::PathBuf;
use suggest::SuggestionEngine;
use ui::{App, Overlay};

#[derive(Parser, Debug)]
#[command(
    name = "cosmos",
    about = "A contemplative vibe coding companion",
    long_about = "☽ C O S M O S ✦\n\n\
                  A contemplative companion for your codebase.\n\n\
                  Uses AST-based indexing and AI to suggest improvements,\n\
                  bug fixes, features, and optimizations.",
    version
)]
struct Args {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Set up OpenRouter API key for AI features
    #[arg(long)]
    setup: bool,

    /// Show stats and exit (no TUI)
    #[arg(long)]
    stats: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --setup flag
    if args.setup {
        return setup_api_key();
    }

    let path = args.path.canonicalize()?;

    // Show startup message
    eprintln!();
    eprintln!("  ☽ C O S M O S ✦");
    eprintln!("  a contemplative companion for your codebase");
    eprintln!();

    // Initialize with progress indication
    let index = init_index(&path)?;
    let context = init_context(&path)?;
    let suggestions = SuggestionEngine::new(index.clone());

    // Stats mode: print and exit
    if args.stats {
        print_stats(&index, &suggestions, &context);
        return Ok(());
    }

    // Run TUI
    run_tui(index, suggestions, context)
}

/// Initialize the codebase index
fn init_index(path: &PathBuf) -> Result<CodebaseIndex> {
    eprint!("  Indexing codebase...");
    
    let index = CodebaseIndex::new(path)?;
    let stats = index.stats();
    
    eprintln!(
        " {} files, {} symbols, {} patterns",
        stats.file_count,
        stats.symbol_count,
        stats.pattern_count
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
    println!("  ║           ☽ C O S M O S ✦ Stats                  ║");
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
    println!("  ✓ API key configured. You can now use AI features!");
    Ok(())
}

/// Run the TUI application
fn run_tui(index: CodebaseIndex, suggestions: SuggestionEngine, context: WorkContext) -> Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(index, suggestions, context);

    // Main loop
    let result = run_loop(&mut terminal, &mut app);

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

/// Main event loop
fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        // Clear expired toasts
        app.clear_expired_toast();

        // Render
        terminal.draw(|f| ui::render(f, app))?;

        // Handle events
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Handle overlay mode
            if app.overlay != Overlay::None {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                    KeyCode::Down | KeyCode::Char('j') => app.overlay_scroll_down(),
                    KeyCode::Up | KeyCode::Char('k') => app.overlay_scroll_up(),
                    KeyCode::Char('a') => {
                        // Apply from suggestion detail
                        if let Overlay::SuggestionDetail { suggestion_id, .. } = &app.overlay {
                            // TODO: Implement apply
                            app.show_toast("Apply coming soon...");
                            app.close_overlay();
                        }
                    }
                    KeyCode::Char('d') => {
                        // Dismiss from suggestion detail
                        if let Overlay::SuggestionDetail { suggestion_id, .. } = &app.overlay {
                            let id = *suggestion_id;
                            app.suggestions.dismiss(id);
                            app.show_toast("Dismissed");
                            app.close_overlay();
                        }
                    }
                    KeyCode::Char('y') => {
                        // Confirm apply
                        if let Overlay::ApplyConfirm { suggestion_id, .. } = &app.overlay {
                            let id = *suggestion_id;
                            app.suggestions.mark_applied(id);
                            app.show_toast("Applied!");
                            app.close_overlay();
                        }
                    }
                    KeyCode::Char('n') => {
                        // Cancel apply
                        if matches!(app.overlay, Overlay::ApplyConfirm { .. }) {
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
                KeyCode::Esc => {
                    if app.overlay != Overlay::None {
                        app.close_overlay();
                    }
                }
                KeyCode::Tab => app.toggle_panel(),
                KeyCode::Down | KeyCode::Char('j') => app.navigate_down(),
                KeyCode::Up | KeyCode::Char('k') => app.navigate_up(),
                KeyCode::Enter => app.show_suggestion_detail(),
                KeyCode::Char('?') => app.toggle_help(),
                KeyCode::Char('d') => app.dismiss_selected(),
                KeyCode::Char('a') => {
                    // Apply selected suggestion
                    if let Some(suggestion) = app.selected_suggestion() {
                        app.show_toast("Apply coming soon...");
                    }
                }
                KeyCode::Char('i') => {
                    // Inquiry - request AI suggestions
                    if !suggest::llm::is_available() {
                        app.show_toast("Run: cosmos --setup");
                    } else {
                        app.show_toast("Inquiry coming soon...");
                    }
                }
                KeyCode::Char('r') => {
                    // Refresh context
                    if let Err(e) = app.context.refresh() {
                        app.show_toast(&format!("Refresh failed: {}", e));
                    } else {
                        app.show_toast("Refreshed");
                    }
                }
                _ => {}
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
