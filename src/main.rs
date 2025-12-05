mod analysis;
mod history;
mod prompt;
mod score;
mod spinner;
mod ui;

use analysis::{
    AuthorAnalyzer, ComplexityAnalyzer, GitAnalyzer, StalenessAnalyzer, TestAnalyzer, TodoScanner,
};
use prompt::PromptBuilder;
use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use history::ScoreHistory;
use ratatui::prelude::*;
use score::{HealthScore, RepoMetrics};
use serde::Serialize;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use ui::{App, Overlay};

#[derive(Parser, Debug)]
#[command(
    name = "codecosmos",
    about = "A terminal health dashboard for your codebase",
    long_about = "codecosmos - A sophisticated TUI for codebase health analysis.\n\n\
                  Analyze code complexity, churn, technical debt, test coverage,\n\
                  bus factor risk, and more. Get an instant health score (0-100)\n\
                  for any git repository.",
    version
)]
struct Args {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Number of days to analyze for churn (default: 14)
    #[arg(short, long, default_value = "14")]
    days: i64,

    /// Minimum days for a file to be considered dusty (default: 90)
    #[arg(short = 's', long, default_value = "90")]
    stale_days: i64,

    /// Print health summary and exit (no TUI)
    #[arg(short, long)]
    check: bool,

    /// Minimum health score threshold (exit code 1 if below)
    #[arg(short = 't', long)]
    threshold: Option<u8>,

    /// Output results as JSON
    #[arg(long)]
    json: bool,

    /// Save current score to history
    #[arg(long)]
    save: bool,

    /// Skip bus factor analysis (faster but less data)
    #[arg(long)]
    skip_authors: bool,
}

/// JSON output structure for --json flag
#[derive(Serialize)]
struct JsonOutput {
    score: u8,
    grade: String,
    components: ComponentsOutput,
    metrics: MetricsOutput,
    danger_zones: Vec<DangerZoneOutput>,
    test_coverage: Option<TestCoverageOutput>,
    bus_factor: Option<BusFactorOutput>,
}

#[derive(Serialize)]
struct ComponentsOutput {
    churn: u8,
    complexity: u8,
    debt: u8,
    freshness: u8,
}

#[derive(Serialize)]
struct MetricsOutput {
    total_files: usize,
    total_loc: usize,
    files_changed_recently: usize,
    todo_count: usize,
    fixme_count: usize,
    hack_count: usize,
    dusty_file_count: usize,
    danger_zone_count: usize,
}

#[derive(Serialize)]
struct DangerZoneOutput {
    path: String,
    danger_score: f64,
    change_count: usize,
    complexity_score: f64,
}

#[derive(Serialize)]
struct TestCoverageOutput {
    coverage_pct: f64,
    files_with_tests: usize,
    files_without_tests: usize,
    untested_danger_zones: Vec<String>,
}

#[derive(Serialize)]
struct BusFactorOutput {
    total_authors: usize,
    single_author_files: usize,
    avg_bus_factor: f64,
    high_risk_files: Vec<BusRiskOutput>,
}

#[derive(Serialize)]
struct BusRiskOutput {
    path: String,
    primary_author: String,
    primary_author_pct: f64,
}

fn main() -> ExitCode {
    match run() {
        Ok(passed) => {
            if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<bool> {
    let args = Args::parse();
    let path = args.path.canonicalize()?;

    // Analyze the repository with animated progress
    let git_analyzer = GitAnalyzer::new(&path)?;
    let staleness_analyzer = StalenessAnalyzer::new(&path)?;
    let todo_scanner = TodoScanner::new();
    let complexity_analyzer = ComplexityAnalyzer::new();
    let test_analyzer = TestAnalyzer::new();

    let repo_name = git_analyzer.repo_name();
    let branch_name = git_analyzer.current_branch()?;

    // Use animated spinner for analysis phase
    let use_spinner = !args.json && !args.check;
    let mut spin = if use_spinner {
        spinner::print_analysis_header(&repo_name);
        let s = spinner::Spinner::new(spinner::SpinnerStyle::Circle)
            .with_message("analyzing churn...");
        s.start();
        Some(s)
    } else {
        if !args.json {
            eprintln!(":: Analyzing repository...");
            eprintln!("   → churn");
        }
        None
    };

    let churn_entries = git_analyzer.analyze_churn(args.days)?;
    let commits_recent = git_analyzer.commit_count(args.days)?;

    if let Some(ref mut s) = spin {
        s.set_message("analyzing complexity...");
        s.tick();
    } else if !args.json {
        eprintln!("   → complexity");
    }
    let complexity_entries = complexity_analyzer.analyze(&path)?;
    let (total_loc, avg_complexity, max_complexity) =
        complexity_analyzer.aggregate_stats(&complexity_entries);

    if let Some(ref mut s) = spin {
        s.set_message("finding danger zones...");
        s.tick();
    } else if !args.json {
        eprintln!("   → danger zones");
    }
    let danger_zones =
        complexity_analyzer.find_danger_zones(&churn_entries, &complexity_entries, 20);

    if let Some(ref mut s) = spin {
        s.set_message("checking staleness...");
        s.tick();
    } else if !args.json {
        eprintln!("   → staleness");
    }
    let dusty_files = staleness_analyzer.find_dusty_files(args.stale_days)?;
    let total_files = staleness_analyzer.total_file_count()?;

    if let Some(ref mut s) = spin {
        s.set_message("scanning debt markers...");
        s.tick();
    } else if !args.json {
        eprintln!("   → debt markers");
    }
    let todo_entries = todo_scanner.scan(&path)?;

    if let Some(ref mut s) = spin {
        s.set_message("analyzing test coverage...");
        s.tick();
    } else if !args.json {
        eprintln!("   → test coverage");
    }
    let test_coverages = test_analyzer.analyze(&path)?;
    let danger_zone_paths: Vec<String> = danger_zones.iter().map(|d| d.path.clone()).collect();
    let test_summary = test_analyzer.summarize(&test_coverages, &danger_zone_paths);

    // Bus factor analysis (optional, can be slow on large repos)
    let (bus_factor_risks, author_stats) = if !args.skip_authors {
        if let Some(ref mut s) = spin {
            s.set_message("analyzing bus factor...");
            s.tick();
        } else if !args.json {
            eprintln!("   → bus factor");
        }
        let author_analyzer = AuthorAnalyzer::new(&path)?;
        let authorships = author_analyzer.analyze(&path, args.days)?;
        let risks = author_analyzer.find_high_risk_files(&authorships, 50);
        let stats = author_analyzer.aggregate_stats(&authorships, args.days)?;
        (risks, Some(stats))
    } else {
        (Vec::new(), None)
    };

    // Finish spinner
    if let Some(s) = spin {
        s.finish_with_message("analysis complete");
    }

    // Calculate metrics and score
    let metrics = RepoMetrics::from_analysis(
        total_files,
        total_loc,
        &churn_entries,
        &todo_entries,
        &dusty_files,
        commits_recent,
        avg_complexity,
        max_complexity,
        danger_zones.len(),
    );

    // Load history and calculate trend
    let mut history = ScoreHistory::load(&path).unwrap_or_default();
    let previous_score = history.latest_score();
    let history_entries = history.recent_entries(20).to_vec();

    let score = HealthScore::calculate(&metrics).with_trend(previous_score);

    // Save to history if requested
    if args.save {
        history.add_entry(&score, Some(branch_name.clone()));
        if let Err(e) = history.save(&path) {
            if !args.json {
                eprintln!("   !! Failed to save history: {}", e);
            }
        } else if !args.json {
            eprintln!("   → saved to history");
        }
    }

    // Check threshold
    let passes_threshold = args.threshold.map_or(true, |t| score.value >= t);

    // JSON output mode
    if args.json {
        let output = JsonOutput {
            score: score.value,
            grade: score.grade.to_string(),
            components: ComponentsOutput {
                churn: score.components.churn,
                complexity: score.components.complexity,
                debt: score.components.debt,
                freshness: score.components.freshness,
            },
            metrics: MetricsOutput {
                total_files: metrics.total_files,
                total_loc: metrics.total_loc,
                files_changed_recently: metrics.files_changed_recently,
                todo_count: metrics.todo_count,
                fixme_count: metrics.fixme_count,
                hack_count: metrics.hack_count,
                dusty_file_count: metrics.dusty_file_count,
                danger_zone_count: metrics.danger_zone_count,
            },
            danger_zones: danger_zones
                .iter()
                .map(|dz| DangerZoneOutput {
                    path: dz.path.clone(),
                    danger_score: dz.danger_score,
                    change_count: dz.change_count,
                    complexity_score: dz.complexity_score,
                })
                .collect(),
            test_coverage: Some(TestCoverageOutput {
                coverage_pct: test_summary.coverage_pct,
                files_with_tests: test_summary.files_with_tests,
                files_without_tests: test_summary.files_without_tests,
                untested_danger_zones: test_summary.untested_danger_zones.clone(),
            }),
            bus_factor: author_stats.as_ref().map(|s| BusFactorOutput {
                total_authors: s.total_authors,
                single_author_files: s.single_author_files,
                avg_bus_factor: s.avg_bus_factor,
                high_risk_files: bus_factor_risks
                    .iter()
                    .take(10)
                    .map(|r| BusRiskOutput {
                        path: r.path.clone(),
                        primary_author: r.primary_author.clone(),
                        primary_author_pct: r.primary_author_pct,
                    })
                    .collect(),
            }),
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(passes_threshold);
    }

    // Check mode: print summary and exit
    if args.check {
        print_summary(
            &score,
            &metrics,
            &repo_name,
            &branch_name,
            &danger_zones,
            &test_summary,
            author_stats.as_ref(),
            args.threshold,
        );
        return Ok(passes_threshold);
    }

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create prompt builder for clipboard support
    let prompt_builder = PromptBuilder::new();

    // Create app with all data
    let mut app = App::new(
        score,
        metrics,
        repo_name,
        branch_name,
        churn_entries,
        dusty_files,
        todo_entries,
        danger_zones,
    )
    .with_tests(test_coverages, test_summary)
    .with_history(history_entries)
    .with_complexity(complexity_entries)
    .with_prompt_builder(prompt_builder);

    // Add bus factor data if available
    if let Some(stats) = author_stats {
        app = app.with_bus_factor(bus_factor_risks, stats);
    }

    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {}", err);
    }

    Ok(passes_threshold)
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        // Clear expired toasts
        app.clear_expired_toast();
        
        terminal.draw(|f| ui::render(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Handle search input mode
            if app.search_active {
                match key.code {
                    KeyCode::Esc => app.end_search(),
                    KeyCode::Enter => app.end_search(),
                    KeyCode::Backspace => app.search_backspace(),
                    KeyCode::Char(c) => app.search_input(c),
                    _ => {}
                }
                continue;
            }

            // Handle overlay mode
            if app.overlay != Overlay::None {
                match &app.overlay {
                    Overlay::ActionMenu => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                            KeyCode::Char('p') => {
                                app.close_overlay();
                                app.generate_prompt_for_selected();
                            }
                            KeyCode::Char('P') => {
                                app.close_overlay();
                                app.generate_batch_prompt();
                            }
                            KeyCode::Char('c') => {
                                app.close_overlay();
                                app.copy_file_path();
                            }
                            _ => {}
                        }
                    }
                    Overlay::PromptCopied(_) => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.close_overlay(),
                            _ => {}
                        }
                    }
                    Overlay::Help => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                            KeyCode::Char('?') => app.toggle_help(),
                            _ => {}
                        }
                    }
                    Overlay::None => {}
                }
                continue;
            }

            // Normal mode
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.should_quit = true;
                }
                KeyCode::Char('1') => app.select_panel(0),
                KeyCode::Char('2') => app.select_panel(1),
                KeyCode::Char('3') => app.select_panel(2),
                KeyCode::Char('4') => app.select_panel(3),
                KeyCode::Char('5') => app.select_panel(4),
                KeyCode::Char('6') => app.select_panel(5),
                KeyCode::Tab => app.next_panel(),
                KeyCode::BackTab => app.prev_panel(),
                KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                KeyCode::PageDown | KeyCode::Char('d') => app.page_down(),
                KeyCode::PageUp | KeyCode::Char('u') => app.page_up(),
                KeyCode::Home | KeyCode::Char('g') => {
                    app.scroll_offset = 0;
                    app.selected_file_index = Some(0);
                }
                KeyCode::End | KeyCode::Char('G') => {
                    let len = app.danger_zones.len().max(1);
                    app.scroll_offset = len.saturating_sub(15);
                    app.selected_file_index = Some(len.saturating_sub(1));
                }
                KeyCode::Char('/') => app.start_search(),
                KeyCode::Char('?') => app.toggle_help(),
                KeyCode::Enter => app.show_action_menu(),
                // Prompt builder shortcuts
                KeyCode::Char('p') => app.generate_prompt_for_selected(),
                KeyCode::Char('P') => app.generate_batch_prompt(),
                KeyCode::Char('c') => app.copy_file_path(),
                _ => {}
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn print_summary(
    score: &HealthScore,
    metrics: &RepoMetrics,
    repo_name: &str,
    branch_name: &str,
    danger_zones: &[analysis::DangerZone],
    test_summary: &analysis::TestSummary,
    author_stats: Option<&analysis::AuthorStats>,
    threshold: Option<u8>,
) {
    let total_todos = metrics.todo_count + metrics.fixme_count + metrics.hack_count;

    // Determine visual indicator based on score
    let score_indicator = if score.value >= 75 { "●" } else if score.value >= 60 { "◐" } else { "○" };

    let trend_str = match score.trend {
        score::Trend::Improving => " ↑",
        score::Trend::Declining => " ↓",
        score::Trend::Stable => " →",
        score::Trend::Unknown => "",
    };

    println!();
    println!("┌─────────────────────────────────────────────────────────────────┐");
    println!(
        "│  {} {}/100 ({}){}                                               │",
        score_indicator, score.value, score.grade, trend_str
    );
    println!(
        "│  \"{}\"{}│",
        score.grade.description(),
        " ".repeat(47 - score.grade.description().len())
    );
    println!("│                                                                 │");
    println!(
        "│  {} @ {}{}│",
        repo_name,
        branch_name,
        " ".repeat(52 - repo_name.len() - branch_name.len())
    );
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!(
        "│  files: {:4}   danger: {:3}   todos: {:3}   dusty: {:3}          │",
        metrics.total_files, metrics.danger_zone_count, total_todos, metrics.dusty_file_count
    );
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│  Components:                                                    │");
    println!(
        "│    churn: {:3}   complexity: {:3}   debt: {:3}   freshness: {:3}   │",
        score.components.churn,
        score.components.complexity,
        score.components.debt,
        score.components.freshness
    );
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!(
        "│  Test coverage: {:.0}% ({} tested, {} untested)                     │",
        test_summary.coverage_pct, test_summary.files_with_tests, test_summary.files_without_tests
    );
    if let Some(stats) = author_stats {
        println!(
            "│  Bus factor: {:.1} avg ({} single-author files)                   │",
            stats.avg_bus_factor, stats.single_author_files
        );
    }
    println!("└─────────────────────────────────────────────────────────────────┘");

    if !danger_zones.is_empty() {
        println!();
        println!("DANGER ZONES - files that are both complex AND frequently changed:");
        println!();
        for (i, dz) in danger_zones.iter().take(5).enumerate() {
            let risk_indicator = if dz.danger_score >= 70.0 {
                "▓▓"
            } else if dz.danger_score >= 50.0 {
                "▓░"
            } else {
                "░░"
            };
            println!("  {}. {} {}", i + 1, risk_indicator, dz.path);
            println!(
                "     {} changes │ complexity {:.1} │ {}",
                dz.change_count, dz.complexity_score, dz.reason
            );
            println!();
        }
    }

    if !test_summary.untested_danger_zones.is_empty() {
        println!("⚠  UNTESTED DANGER ZONES:");
        for path in test_summary.untested_danger_zones.iter().take(3) {
            println!("   ○ {}", path);
        }
        println!();
    }

    if let Some(t) = threshold {
        println!();
        if score.value >= t {
            println!("● PASS - Score {} meets threshold {}", score.value, t);
        } else {
            println!(
                "○ FAIL - Score {} is below threshold {} (need +{})",
                score.value,
                t,
                t - score.value
            );
        }
    }

    println!();
}

#[allow(dead_code)]
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        let start = path.len() - max_len + 3;
        format!("...{}", &path[start..])
    }
}
