pub mod panels;
pub mod theme;

use crate::analysis::{
    AuthorStats, BusFactorRisk, ChurnEntry, DangerZone, DustyFile, FileComplexity, TestCoverage,
    TestSummary, TodoEntry,
};
use crate::history::HistoryEntry;
use crate::prompt::{FileContext, IssueType, PromptBuilder};
use crate::score::{HealthScore, RepoMetrics, Trend};
use std::time::Instant;
use panels::Panel;
use theme::{sparkline, Theme};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
    Frame,
};

/// The active panel in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePanel {
    #[default]
    DangerZones,
    Hotspots,
    DustyFiles,
    Todos,
    BusFactor,
    Tests,
}

impl ActivePanel {
    pub fn index(&self) -> usize {
        match self {
            ActivePanel::DangerZones => 0,
            ActivePanel::Hotspots => 1,
            ActivePanel::DustyFiles => 2,
            ActivePanel::Todos => 3,
            ActivePanel::BusFactor => 4,
            ActivePanel::Tests => 5,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            0 => ActivePanel::DangerZones,
            1 => ActivePanel::Hotspots,
            2 => ActivePanel::DustyFiles,
            3 => ActivePanel::Todos,
            4 => ActivePanel::BusFactor,
            5 => ActivePanel::Tests,
            _ => ActivePanel::DangerZones,
        }
    }

    pub fn count() -> usize {
        6
    }
}

/// UI overlay state
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    None,
    Help,
    ActionMenu,
    PromptCopied(String), // Contains a preview of the copied prompt
}

/// Toast notification
pub struct Toast {
    pub message: String,
    pub created_at: Instant,
}

impl Toast {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            created_at: Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() >= 3
    }
}

/// Main application state
pub struct App {
    pub score: HealthScore,
    pub metrics: RepoMetrics,
    pub repo_name: String,
    pub branch_name: String,
    pub churn_entries: Vec<ChurnEntry>,
    pub dusty_files: Vec<DustyFile>,
    pub todo_entries: Vec<TodoEntry>,
    pub danger_zones: Vec<DangerZone>,
    pub bus_factor_risks: Vec<BusFactorRisk>,
    pub author_stats: Option<AuthorStats>,
    pub test_coverages: Vec<TestCoverage>,
    pub test_summary: Option<TestSummary>,
    pub complexity_entries: Vec<FileComplexity>,
    pub history_entries: Vec<HistoryEntry>,
    pub active_panel: ActivePanel,
    pub scroll_offset: usize,
    pub should_quit: bool,
    pub search_query: String,
    pub search_active: bool,
    pub overlay: Overlay,
    pub selected_file_index: Option<usize>,
    pub prompt_builder: Option<PromptBuilder>,
    pub toast: Option<Toast>,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        score: HealthScore,
        metrics: RepoMetrics,
        repo_name: String,
        branch_name: String,
        churn_entries: Vec<ChurnEntry>,
        dusty_files: Vec<DustyFile>,
        todo_entries: Vec<TodoEntry>,
        danger_zones: Vec<DangerZone>,
    ) -> Self {
        Self {
            score,
            metrics,
            repo_name,
            branch_name,
            churn_entries,
            dusty_files,
            todo_entries,
            danger_zones,
            bus_factor_risks: Vec::new(),
            author_stats: None,
            test_coverages: Vec::new(),
            test_summary: None,
            complexity_entries: Vec::new(),
            history_entries: Vec::new(),
            active_panel: ActivePanel::default(),
            scroll_offset: 0,
            should_quit: false,
            search_query: String::new(),
            search_active: false,
            overlay: Overlay::None,
            selected_file_index: Some(0),
            prompt_builder: None,
            toast: None,
        }
    }

    pub fn with_bus_factor(mut self, risks: Vec<BusFactorRisk>, stats: AuthorStats) -> Self {
        self.bus_factor_risks = risks;
        self.author_stats = Some(stats);
        self
    }

    pub fn with_tests(mut self, coverages: Vec<TestCoverage>, summary: TestSummary) -> Self {
        self.test_coverages = coverages;
        self.test_summary = Some(summary);
        self
    }

    pub fn with_history(mut self, entries: Vec<HistoryEntry>) -> Self {
        self.history_entries = entries;
        self
    }

    pub fn with_complexity(mut self, entries: Vec<FileComplexity>) -> Self {
        self.complexity_entries = entries;
        self
    }

    pub fn with_prompt_builder(mut self, builder: PromptBuilder) -> Self {
        self.prompt_builder = Some(builder);
        self
    }

    pub fn next_panel(&mut self) {
        self.active_panel =
            ActivePanel::from_index((self.active_panel.index() + 1) % ActivePanel::count());
        self.scroll_offset = 0;
        self.selected_file_index = Some(0);
    }

    pub fn prev_panel(&mut self) {
        self.active_panel = ActivePanel::from_index(
            (self.active_panel.index() + ActivePanel::count() - 1) % ActivePanel::count(),
        );
        self.scroll_offset = 0;
        self.selected_file_index = Some(0);
    }

    pub fn select_panel(&mut self, index: usize) {
        if index < ActivePanel::count() {
            self.active_panel = ActivePanel::from_index(index);
            self.scroll_offset = 0;
            self.selected_file_index = Some(0);
        }
    }

    pub fn scroll_down(&mut self) {
        let max_scroll = self.current_panel_len().saturating_sub(1);
        if let Some(idx) = self.selected_file_index {
            self.selected_file_index = Some((idx + 1).min(max_scroll));
        }
        // Adjust scroll offset to keep selection visible
        if let Some(idx) = self.selected_file_index {
            if idx >= self.scroll_offset + 15 {
                self.scroll_offset = idx.saturating_sub(14);
            }
        }
    }

    pub fn scroll_up(&mut self) {
        if let Some(idx) = self.selected_file_index {
            self.selected_file_index = Some(idx.saturating_sub(1));
        }
        // Adjust scroll offset to keep selection visible
        if let Some(idx) = self.selected_file_index {
            if idx < self.scroll_offset {
                self.scroll_offset = idx;
            }
        }
    }

    pub fn page_down(&mut self) {
        let max_scroll = self.current_panel_len().saturating_sub(1);
        if let Some(idx) = self.selected_file_index {
            self.selected_file_index = Some((idx + 10).min(max_scroll));
            self.scroll_offset = (self.scroll_offset + 10).min(max_scroll.saturating_sub(14));
        }
    }

    pub fn page_up(&mut self) {
        if let Some(idx) = self.selected_file_index {
            self.selected_file_index = Some(idx.saturating_sub(10));
            self.scroll_offset = self.scroll_offset.saturating_sub(10);
        }
    }

    pub fn toggle_help(&mut self) {
        self.overlay = match self.overlay {
            Overlay::Help => Overlay::None,
            _ => Overlay::Help,
        };
    }

    pub fn show_action_menu(&mut self) {
        self.overlay = match self.overlay {
            Overlay::ActionMenu => Overlay::None,
            _ => Overlay::ActionMenu,
        };
    }

    pub fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
    }

    /// Build a FileContext for the currently selected file with FULL context
    fn build_file_context(&self) -> Option<FileContext> {
        let path = self.selected_file_path()?;
        let mut ctx = FileContext::new(&path);

        // Add data from each source
        if let Some(dz) = self.danger_zones.iter().find(|d| d.path == path) {
            ctx = ctx.with_danger_zone(dz);
        }
        if let Some(churn) = self.churn_entries.iter().find(|c| c.path == path) {
            ctx = ctx.with_churn(churn);
        }
        if let Some(fc) = self.complexity_entries.iter().find(|c| c.path == path) {
            ctx = ctx.with_complexity(fc);
        }
        if let Some(df) = self.dusty_files.iter().find(|d| d.path == path) {
            ctx = ctx.with_dusty(df);
        }
        if let Some(bf) = self.bus_factor_risks.iter().find(|b| b.path == path) {
            ctx = ctx.with_bus_factor(bf);
        }
        if let Some(tc) = self.test_coverages.iter().find(|t| t.path == path) {
            ctx = ctx.with_test_coverage(tc);
        }

        // Add TODOs from this file
        ctx = ctx.with_todos_from_list(&self.todo_entries);

        // Set issue type based on active panel if not already set
        if ctx.issue_type.is_none() {
            ctx.issue_type = Some(match self.active_panel {
                ActivePanel::DangerZones => IssueType::DangerZone,
                ActivePanel::Hotspots => IssueType::HighChurn,
                ActivePanel::DustyFiles => IssueType::DustyFile,
                ActivePanel::Todos => IssueType::TodoItem,
                ActivePanel::BusFactor => IssueType::BusFactorRisk,
                ActivePanel::Tests => IssueType::MissingTests,
            });
        }

        // Load the actual file content - THIS IS THE KEY!
        ctx.load_file_content();

        Some(ctx)
    }

    /// Generate AI prompt for the selected file and copy to clipboard
    pub fn generate_prompt_for_selected(&mut self) {
        if let Some(ctx) = self.build_file_context() {
            if let Some(ref mut builder) = self.prompt_builder {
                match builder.generate_and_copy(&ctx) {
                    Ok(prompt) => {
                        // Show first few lines as preview
                        let preview: String = prompt.lines().take(5).collect::<Vec<_>>().join("\n");
                        self.overlay = Overlay::PromptCopied(preview);
                    }
                    Err(e) => {
                        self.toast = Some(Toast::new(&format!("Error: {}", e)));
                    }
                }
            }
        }
    }

    /// Generate batch prompt for current panel - with FULL file content!
    pub fn generate_batch_prompt(&mut self) {
        let mut contexts: Vec<FileContext> = match self.active_panel {
            ActivePanel::DangerZones => self
                .danger_zones
                .iter()
                .take(5) // Limit to 5 for batch to keep prompt manageable
                .map(|dz| {
                    let mut ctx = FileContext::new(&dz.path).with_danger_zone(dz);
                    // Enrich with other data
                    if let Some(fc) = self.complexity_entries.iter().find(|c| c.path == dz.path) {
                        ctx = ctx.with_complexity(fc);
                    }
                    if let Some(tc) = self.test_coverages.iter().find(|t| t.path == dz.path) {
                        ctx = ctx.with_test_coverage(tc);
                    }
                    if let Some(bf) = self.bus_factor_risks.iter().find(|b| b.path == dz.path) {
                        ctx = ctx.with_bus_factor(bf);
                    }
                    ctx = ctx.with_todos_from_list(&self.todo_entries);
                    ctx
                })
                .collect(),
            ActivePanel::Hotspots => self
                .churn_entries
                .iter()
                .take(5)
                .map(|c| {
                    let mut ctx = FileContext::new(&c.path).with_churn(c);
                    if let Some(fc) = self.complexity_entries.iter().find(|x| x.path == c.path) {
                        ctx = ctx.with_complexity(fc);
                    }
                    ctx = ctx.with_todos_from_list(&self.todo_entries);
                    ctx
                })
                .collect(),
            ActivePanel::DustyFiles => self
                .dusty_files
                .iter()
                .take(5)
                .map(|d| {
                    let mut ctx = FileContext::new(&d.path).with_dusty(d);
                    ctx = ctx.with_todos_from_list(&self.todo_entries);
                    ctx
                })
                .collect(),
            ActivePanel::BusFactor => self
                .bus_factor_risks
                .iter()
                .take(5)
                .map(|b| {
                    let mut ctx = FileContext::new(&b.path).with_bus_factor(b);
                    if let Some(tc) = self.test_coverages.iter().find(|t| t.path == b.path) {
                        ctx = ctx.with_test_coverage(tc);
                    }
                    ctx = ctx.with_todos_from_list(&self.todo_entries);
                    ctx
                })
                .collect(),
            ActivePanel::Tests => self
                .test_coverages
                .iter()
                .filter(|t| !t.has_tests)
                .take(5)
                .map(|t| {
                    let mut ctx = FileContext::new(&t.path).with_test_coverage(t);
                    if let Some(fc) = self.complexity_entries.iter().find(|c| c.path == t.path) {
                        ctx = ctx.with_complexity(fc);
                    }
                    ctx = ctx.with_todos_from_list(&self.todo_entries);
                    ctx
                })
                .collect(),
            ActivePanel::Todos => self
                .todo_entries
                .iter()
                .take(5)
                .map(|t| {
                    let ctx = FileContext::new(&t.path)
                        .with_todo(t)
                        .with_todos_from_list(&self.todo_entries);
                    ctx
                })
                .collect(),
        };

        if contexts.is_empty() {
            self.toast = Some(Toast::new("No items to generate prompt for"));
            return;
        }

        // Load file content for each context!
        for ctx in &mut contexts {
            ctx.load_file_content();
        }

        let panel_name = match self.active_panel {
            ActivePanel::DangerZones => "Danger Zones",
            ActivePanel::Hotspots => "Hotspots",
            ActivePanel::DustyFiles => "Dusty Files",
            ActivePanel::Todos => "TODOs",
            ActivePanel::BusFactor => "Bus Factor",
            ActivePanel::Tests => "Missing Tests",
        };

        let prompt = crate::prompt::generate_batch_prompt(&contexts, panel_name);

        if let Some(ref mut builder) = self.prompt_builder {
            match builder.copy_to_clipboard(&prompt) {
                Ok(_) => {
                    let preview: String = prompt.lines().take(8).collect::<Vec<_>>().join("\n");
                    self.overlay = Overlay::PromptCopied(preview);
                }
                Err(e) => {
                    self.toast = Some(Toast::new(&format!("Error: {}", e)));
                }
            }
        }
    }

    /// Copy the selected file path to clipboard
    pub fn copy_file_path(&mut self) {
        if let Some(path) = self.selected_file_path() {
            if let Some(ref mut builder) = self.prompt_builder {
                match builder.copy_to_clipboard(&path) {
                    Ok(_) => {
                        self.toast = Some(Toast::new(&format!("Copied: {}", path)));
                    }
                    Err(e) => {
                        self.toast = Some(Toast::new(&format!("Error: {}", e)));
                    }
                }
            }
        }
    }

    /// Clear expired toast
    pub fn clear_expired_toast(&mut self) {
        if let Some(ref toast) = self.toast {
            if toast.is_expired() {
                self.toast = None;
            }
        }
    }

    pub fn start_search(&mut self) {
        self.search_active = true;
        self.search_query.clear();
    }

    pub fn end_search(&mut self) {
        self.search_active = false;
    }

    pub fn search_input(&mut self, c: char) {
        self.search_query.push(c);
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
    }

    fn current_panel_len(&self) -> usize {
        if self.search_query.is_empty() {
            match self.active_panel {
                ActivePanel::DangerZones => self.danger_zones.len(),
                ActivePanel::Hotspots => self.churn_entries.len(),
                ActivePanel::DustyFiles => self.dusty_files.len(),
                ActivePanel::Todos => self.todo_entries.len(),
                ActivePanel::BusFactor => self.bus_factor_risks.len(),
                ActivePanel::Tests => self.test_coverages.len(),
            }
        } else {
            let q = self.search_query.to_lowercase();
            match self.active_panel {
                ActivePanel::DangerZones => self
                    .danger_zones
                    .iter()
                    .filter(|d| d.path.to_lowercase().contains(&q))
                    .count(),
                ActivePanel::Hotspots => self
                    .churn_entries
                    .iter()
                    .filter(|c| c.path.to_lowercase().contains(&q))
                    .count(),
                ActivePanel::DustyFiles => self
                    .dusty_files
                    .iter()
                    .filter(|d| d.path.to_lowercase().contains(&q))
                    .count(),
                ActivePanel::Todos => self
                    .todo_entries
                    .iter()
                    .filter(|t| t.path.to_lowercase().contains(&q) || t.text.to_lowercase().contains(&q))
                    .count(),
                ActivePanel::BusFactor => self
                    .bus_factor_risks
                    .iter()
                    .filter(|b| b.path.to_lowercase().contains(&q) || b.primary_author.to_lowercase().contains(&q))
                    .count(),
                ActivePanel::Tests => self
                    .test_coverages
                    .iter()
                    .filter(|t| t.path.to_lowercase().contains(&q))
                    .count(),
            }
        }
    }

    /// Get the currently selected file path
    pub fn selected_file_path(&self) -> Option<String> {
        let idx = self.selected_file_index?;

        match self.active_panel {
            ActivePanel::DangerZones => self.danger_zones.get(idx).map(|d| d.path.clone()),
            ActivePanel::Hotspots => self.churn_entries.get(idx).map(|c| c.path.clone()),
            ActivePanel::DustyFiles => self.dusty_files.get(idx).map(|d| d.path.clone()),
            ActivePanel::Todos => self.todo_entries.get(idx).map(|t| t.path.clone()),
            ActivePanel::BusFactor => self.bus_factor_risks.get(idx).map(|b| b.path.clone()),
            ActivePanel::Tests => self.test_coverages.get(idx).map(|t| t.path.clone()),
        }
    }
}

/// Render the entire UI
pub fn render(frame: &mut Frame, app: &App) {
    // Clear with background color
    let bg = Block::default().style(Theme::bg());
    frame.render_widget(bg, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Header with score + breakdown
            Constraint::Length(3),  // Tab bar
            Constraint::Min(8),     // Main panel
            Constraint::Length(1),  // Status bar
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    render_tabs(frame, chunks[1], app);
    render_panel(frame, chunks[2], app);
    render_status_bar(frame, chunks[3], app);

    // Render overlay if active
    match &app.overlay {
        Overlay::Help => render_help_overlay(frame, app),
        Overlay::ActionMenu => render_action_menu_overlay(frame, app),
        Overlay::PromptCopied(preview) => render_prompt_copied_overlay(frame, preview),
        Overlay::None => {}
    }

    // Render toast if present
    if let Some(toast) = &app.toast {
        render_toast(frame, toast);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24), // Score display
            Constraint::Min(30),    // Breakdown bars
            Constraint::Length(30), // Sparkline + info
        ])
        .split(area);

    render_score_block(frame, chunks[0], app);
    render_breakdown(frame, chunks[1], app);
    render_sparkline_block(frame, chunks[2], app);
}

fn render_score_block(frame: &mut Frame, area: Rect, app: &App) {
    let score_color = Theme::score_color(app.score.value);

    let trend_char = match app.score.trend {
        Trend::Improving => "↑",
        Trend::Declining => "↓",
        Trend::Stable => "→",
        Trend::Unknown => " ",
    };

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {} ", app.score.value),
                Style::default()
                    .fg(score_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({})", app.score.grade),
                Style::default().fg(score_color),
            ),
            Span::styled(
                format!(" {}", trend_char),
                Style::default().fg(Theme::GREY_300),
            ),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("\"{}\"", app.score.grade.description()),
                Style::default()
                    .fg(Theme::GREY_400)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]),
        Line::from(""),
    ];

    let block = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" score ", Theme::title()))
            .style(Theme::panel_bg()),
    );

    frame.render_widget(block, area);
}

fn render_breakdown(frame: &mut Frame, area: Rect, app: &App) {
    let c = &app.score.components;

    // Create visual bars for each component
    let bar_width = 20;
    let make_bar = |value: u8, label: &str| -> Line {
        let filled = (value as usize * bar_width) / 100;
        let bar: String = (0..bar_width)
            .map(|i| if i < filled { Theme::BAR_FILLED } else { Theme::BAR_EMPTY })
            .collect();

        Line::from(vec![
            Span::styled(format!(" {:11} ", label), Theme::text_muted()),
            Span::styled(bar, Style::default().fg(Theme::score_color(value))),
            Span::styled(format!(" {:3}", value), Theme::text()),
        ])
    };

    let content = vec![
        make_bar(c.churn, "churn"),
        make_bar(c.complexity, "complexity"),
        make_bar(c.debt, "debt"),
        make_bar(c.freshness, "freshness"),
    ];

    let block = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" breakdown ", Theme::title()))
            .style(Theme::panel_bg()),
    );

    frame.render_widget(block, area);
}

fn render_sparkline_block(frame: &mut Frame, area: Rect, app: &App) {
    // Get recent scores for sparkline
    let scores: Vec<u8> = app.history_entries.iter().map(|e| e.score).collect();
    let spark = if scores.is_empty() {
        "no history".to_string()
    } else {
        sparkline(&scores, 12)
    };

    let delta_str = if app.history_entries.len() >= 2 {
        let current = app.score.value as i16;
        let previous = app.history_entries[app.history_entries.len() - 2].score as i16;
        let delta = current - previous;
        if delta > 0 {
            format!("+{}", delta)
        } else if delta < 0 {
            format!("{}", delta)
        } else {
            "±0".to_string()
        }
    } else {
        String::new()
    };

    let content = vec![
        Line::from(vec![
            Span::styled(
                format!(" {} @ {}", app.repo_name, app.branch_name),
                Theme::text_muted(),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" trend ", Theme::text_dim()),
            Span::styled(spark, Style::default().fg(Theme::GREY_200)),
            Span::styled(format!(" {}", delta_str), Theme::text_muted()),
        ]),
    ];

    let block = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" repo ", Theme::title()))
            .style(Theme::panel_bg()),
    );

    frame.render_widget(block, area);
}

fn render_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let danger_count = app.danger_zones.len();
    let hotspot_count = app.churn_entries.len();
    let dusty_count = app.dusty_files.len();
    let todo_count = app.todo_entries.len();
    let bus_count = app.bus_factor_risks.len();
    let test_untested = app
        .test_coverages
        .iter()
        .filter(|t| !t.has_tests)
        .count();

    let titles = vec![
        Line::from(format!(" 1·danger {} ", danger_count)),
        Line::from(format!(" 2·hotspots {} ", hotspot_count)),
        Line::from(format!(" 3·dusty {} ", dusty_count)),
        Line::from(format!(" 4·todos {} ", todo_count)),
        Line::from(format!(" 5·bus {} ", bus_count)),
        Line::from(format!(" 6·tests {} ", test_untested)),
    ];

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .style(Theme::panel_bg()),
        )
        .select(app.active_panel.index())
        .style(Theme::text_dim())
        .highlight_style(Theme::selected())
        .divider(Span::styled("│", Theme::border()));

    frame.render_widget(tabs, area);
}

fn render_panel(frame: &mut Frame, area: Rect, app: &App) {
    match app.active_panel {
        ActivePanel::DangerZones => {
            let panel = Panel::danger_zones(
                &app.danger_zones,
                app.scroll_offset,
                app.selected_file_index,
                &app.search_query,
            );
            frame.render_widget(panel, area);
        }
        ActivePanel::Hotspots => {
            let panel = Panel::hotspots(
                &app.churn_entries,
                app.scroll_offset,
                app.selected_file_index,
                &app.search_query,
            );
            frame.render_widget(panel, area);
        }
        ActivePanel::DustyFiles => {
            let panel = Panel::dusty_files(
                &app.dusty_files,
                app.scroll_offset,
                app.selected_file_index,
                &app.search_query,
            );
            frame.render_widget(panel, area);
        }
        ActivePanel::Todos => {
            let panel = Panel::todos(
                &app.todo_entries,
                app.scroll_offset,
                app.selected_file_index,
                &app.search_query,
            );
            frame.render_widget(panel, area);
        }
        ActivePanel::BusFactor => {
            let panel = Panel::bus_factor(
                &app.bus_factor_risks,
                app.author_stats.as_ref(),
                app.scroll_offset,
                app.selected_file_index,
                &app.search_query,
            );
            frame.render_widget(panel, area);
        }
        ActivePanel::Tests => {
            let panel = Panel::tests(
                &app.test_coverages,
                app.test_summary.as_ref(),
                app.scroll_offset,
                app.selected_file_index,
                &app.search_query,
            );
            frame.render_widget(panel, area);
        }
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let content = if app.search_active {
        Line::from(vec![
            Span::styled(" /", Theme::key()),
            Span::styled(&app.search_query, Theme::text()),
            Span::styled("█", Style::default().fg(Theme::GREY_300)),
            Span::styled("  (esc to cancel)", Theme::text_dim()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" q", Theme::key()),
            Span::styled(" quit ", Theme::text_dim()),
            Span::styled(Theme::DOT_SEPARATOR.to_string(), Theme::text_dim()),
            Span::styled(" 1-6", Theme::key()),
            Span::styled(" panel ", Theme::text_dim()),
            Span::styled(Theme::DOT_SEPARATOR.to_string(), Theme::text_dim()),
            Span::styled(" p", Theme::key()),
            Span::styled(" prompt ", Theme::text_dim()),
            Span::styled(Theme::DOT_SEPARATOR.to_string(), Theme::text_dim()),
            Span::styled(" P", Theme::key()),
            Span::styled(" batch ", Theme::text_dim()),
            Span::styled(Theme::DOT_SEPARATOR.to_string(), Theme::text_dim()),
            Span::styled(" ↵", Theme::key()),
            Span::styled(" actions ", Theme::text_dim()),
            Span::styled(Theme::DOT_SEPARATOR.to_string(), Theme::text_dim()),
            Span::styled(" ?", Theme::key()),
            Span::styled(" help", Theme::text_dim()),
        ])
    };

    let status = Paragraph::new(content).style(Theme::bg());
    frame.render_widget(status, area);
}

fn render_help_overlay(frame: &mut Frame, _app: &App) {
    let area = centered_rect(60, 70, frame.area());

    // Clear the area first
    frame.render_widget(Clear, area);

    let help_text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  NAVIGATION", Theme::bold()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  1-6      ", Theme::key()),
            Span::styled("Switch to panel", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  Tab      ", Theme::key()),
            Span::styled("Next panel", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  j/↓      ", Theme::key()),
            Span::styled("Move down", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  k/↑      ", Theme::key()),
            Span::styled("Move up", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  PgDn/PgUp", Theme::key()),
            Span::styled("Page down/up", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  g/G      ", Theme::key()),
            Span::styled("Go to top/bottom", Theme::text()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ACTIONS", Theme::bold()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Enter    ", Theme::key()),
            Span::styled("Open action menu", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  p        ", Theme::key()),
            Span::styled("Generate AI prompt (clipboard)", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  P        ", Theme::key()),
            Span::styled("Batch prompt (top 10)", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  c        ", Theme::key()),
            Span::styled("Copy file path", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  /        ", Theme::key()),
            Span::styled("Search/filter", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  Esc      ", Theme::key()),
            Span::styled("Close overlay / cancel", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  ?        ", Theme::key()),
            Span::styled("Toggle this help", Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("  q        ", Theme::key()),
            Span::styled("Quit", Theme::text()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  PANELS", Theme::bold()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  1 danger    ", Theme::text()),
            Span::styled("High churn + high complexity files", Theme::text_dim()),
        ]),
        Line::from(vec![
            Span::styled("  2 hotspots  ", Theme::text()),
            Span::styled("Most frequently changed files", Theme::text_dim()),
        ]),
        Line::from(vec![
            Span::styled("  3 dusty     ", Theme::text()),
            Span::styled("Files untouched for a long time", Theme::text_dim()),
        ]),
        Line::from(vec![
            Span::styled("  4 todos     ", Theme::text()),
            Span::styled("TODO/FIXME/HACK comments", Theme::text_dim()),
        ]),
        Line::from(vec![
            Span::styled("  5 bus       ", Theme::text()),
            Span::styled("Single-author risk (bus factor)", Theme::text_dim()),
        ]),
        Line::from(vec![
            Span::styled("  6 tests     ", Theme::text()),
            Span::styled("Test coverage status", Theme::text_dim()),
        ]),
        Line::from(""),
    ];

    let help = Paragraph::new(help_text).block(
        Block::default()
            .title(Span::styled(" help ", Theme::title()))
            .borders(Borders::ALL)
            .border_style(Theme::border_active())
            .style(Style::default().bg(Theme::GREY_800)),
    );

    frame.render_widget(help, area);
}

fn render_action_menu_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(70, 70, frame.area());

    // Clear the area first
    frame.render_widget(Clear, area);

    let path = app.selected_file_path().unwrap_or_else(|| "No file selected".to_string());

    // Gather all info about this file from different data sources
    let churn_info = app.churn_entries.iter().find(|c| c.path == path);
    let danger_info = app.danger_zones.iter().find(|d| d.path == path);
    let dusty_info = app.dusty_files.iter().find(|d| d.path == path);
    let bus_info = app.bus_factor_risks.iter().find(|b| b.path == path);
    let test_info = app.test_coverages.iter().find(|t| t.path == path);
    let complexity_info = app.complexity_entries.iter().find(|c| c.path == path);

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(&path, Theme::bold()),
        ]),
        Line::from(""),
    ];

    // Status badges
    let mut badges = Vec::new();
    if danger_info.is_some() {
        badges.push(Span::styled(" ▓▓ DANGER ", Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD)));
    }
    if test_info.map_or(false, |t| !t.has_tests) {
        badges.push(Span::styled(" ○ NO TESTS ", Theme::danger_high()));
    }
    if bus_info.is_some() {
        badges.push(Span::styled(" ◐ BUS RISK ", Theme::text_muted()));
    }
    if !badges.is_empty() {
        let mut badge_line = vec![Span::styled("  ", Style::default())];
        badge_line.extend(badges);
        lines.push(Line::from(badge_line));
        lines.push(Line::from(""));
    }

    // Actions section - the star of the show
    lines.push(Line::from(vec![
        Span::styled("  ╭─ ", Style::default().fg(Theme::GREY_500)),
        Span::styled("ACTIONS", Theme::bold()),
        Span::styled(" ─────────────────────────────────────────────────────╮", Style::default().fg(Theme::GREY_500)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
        Span::styled("p", Theme::key()),
        Span::styled("  Generate AI prompt (copy to clipboard)                │", Theme::text()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
        Span::styled("c", Theme::key()),
        Span::styled("  Copy file path                                        │", Theme::text()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
        Span::styled("P", Theme::key()),
        Span::styled("  Generate batch prompt (top 10 in panel)               │", Theme::text()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ╰───────────────────────────────────────────────────────────╯", Style::default().fg(Theme::GREY_500)),
    ]));
    lines.push(Line::from(""));

    // Metrics section
    lines.push(Line::from(vec![
        Span::styled("  ╭─ ", Style::default().fg(Theme::GREY_500)),
        Span::styled("METRICS", Theme::text_muted()),
        Span::styled(" ─────────────────────────────────────────────────────╮", Style::default().fg(Theme::GREY_500)),
    ]));

    if let Some(c) = complexity_info {
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
            Span::styled(format!("Lines: {:4}  │  Functions: {:3}  │  Max fn: {:3} lines", c.loc, c.function_count, c.max_function_length), Theme::text_dim()),
            Span::styled("    │", Style::default().fg(Theme::GREY_500)),
        ]));
    } else if let Some(dusty) = dusty_info {
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
            Span::styled(format!("Lines: {:4}  │  Untouched: {} days", dusty.line_count, dusty.days_since_change), Theme::text_dim()),
            Span::styled("                    │", Style::default().fg(Theme::GREY_500)),
        ]));
    }

    if let Some(churn) = churn_info {
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
            Span::styled(format!("Churn: {}× in {} days", churn.change_count, churn.days_active), Theme::text_dim()),
            Span::styled("                                      │", Style::default().fg(Theme::GREY_500)),
        ]));
    }

    if let Some(danger) = danger_info {
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
            Span::styled(format!("Danger: {:.0}/100  │  Complexity: {:.1}", danger.danger_score, danger.complexity_score), Theme::text_dim()),
            Span::styled("                    │", Style::default().fg(Theme::GREY_500)),
        ]));
    }

    if let Some(bus) = bus_info {
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
            Span::styled(format!("Primary: {} ({:.0}%)", bus.primary_author, bus.primary_author_pct), Theme::text_dim()),
            Span::styled("                                │", Style::default().fg(Theme::GREY_500)),
        ]));
    }

    if let Some(test) = test_info {
        let status = if test.has_tests { "✓ Tested" } else { "✗ No tests" };
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
            Span::styled(format!("Tests: {}", status), if test.has_tests { Theme::text_dim() } else { Theme::danger_high() }),
            Span::styled("                                             │", Style::default().fg(Theme::GREY_500)),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("  ╰───────────────────────────────────────────────────────────╯", Style::default().fg(Theme::GREY_500)),
    ]));

    // Suggestion based on context
    if danger_info.is_some() || test_info.map_or(false, |t| !t.has_tests) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ╭─ ", Style::default().fg(Theme::GREY_500)),
            Span::styled("SUGGESTION", Theme::text_muted()),
            Span::styled(" ──────────────────────────────────────────────────╮", Style::default().fg(Theme::GREY_500)),
        ]));
        
        let suggestion = if danger_info.is_some() && test_info.map_or(false, |t| !t.has_tests) {
            "High priority: Add tests, then refactor for lower complexity"
        } else if danger_info.is_some() {
            "Split long functions, extract helpers, reduce nesting"
        } else if test_info.map_or(false, |t| !t.has_tests) {
            "Add unit tests to enable safe refactoring"
        } else {
            "Review and update if needed"
        };
        
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(Theme::GREY_500)),
            Span::styled(suggestion, Theme::text()),
            Span::styled(" │", Style::default().fg(Theme::GREY_500)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ╰───────────────────────────────────────────────────────────╯", Style::default().fg(Theme::GREY_500)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Press ", Theme::text_dim()),
        Span::styled("p", Theme::key()),
        Span::styled(" to generate prompt · ", Theme::text_dim()),
        Span::styled("Esc", Theme::key()),
        Span::styled(" to close", Theme::text_dim()),
    ]));

    let detail = Paragraph::new(lines).block(
        Block::default()
            .title(Span::styled(" ▸ actions ", Theme::title()))
            .borders(Borders::ALL)
            .border_style(Theme::border_active())
            .style(Style::default().bg(Theme::GREY_800)),
    );

    frame.render_widget(detail, area);
}

fn render_prompt_copied_overlay(frame: &mut Frame, preview: &str) {
    let area = centered_rect(60, 50, frame.area());

    // Clear the area first
    frame.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ✓ ", Style::default().fg(Theme::WHITE)),
            Span::styled("PROMPT COPIED TO CLIPBOARD", Theme::bold()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Paste into your AI assistant to get started.", Theme::text_muted()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ╭─ Preview ──────────────────────────────────────────╮", Style::default().fg(Theme::GREY_500)),
        ]),
    ];

    // Show preview lines
    for line in preview.lines().take(8) {
        let truncated = if line.len() > 52 {
            format!("{}...", &line[..49])
        } else {
            line.to_string()
        };
        lines.push(Line::from(vec![
            Span::styled("  │ ", Style::default().fg(Theme::GREY_500)),
            Span::styled(truncated, Theme::text_dim()),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("  │ ", Style::default().fg(Theme::GREY_500)),
        Span::styled("...", Theme::text_dim()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ╰─────────────────────────────────────────────────────╯", Style::default().fg(Theme::GREY_500)),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Press ", Theme::text_dim()),
        Span::styled("Esc", Theme::key()),
        Span::styled(" to close", Theme::text_dim()),
    ]));

    let overlay = Paragraph::new(lines).block(
        Block::default()
            .title(Span::styled(" ✓ copied ", Theme::title()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::WHITE))
            .style(Style::default().bg(Theme::GREY_800)),
    );

    frame.render_widget(overlay, area);
}

fn render_toast(frame: &mut Frame, toast: &Toast) {
    let area = frame.area();
    
    // Position toast at bottom center
    let toast_width = toast.message.len() as u16 + 6;
    let toast_area = Rect {
        x: (area.width.saturating_sub(toast_width)) / 2,
        y: area.height.saturating_sub(3),
        width: toast_width.min(area.width),
        height: 1,
    };

    let toast_widget = Paragraph::new(Line::from(vec![
        Span::styled(" ✓ ", Style::default().fg(Theme::WHITE)),
        Span::styled(&toast.message, Theme::text()),
        Span::styled(" ", Style::default()),
    ]))
    .style(Style::default().bg(Theme::GREY_600));

    frame.render_widget(toast_widget, toast_area);
}

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
