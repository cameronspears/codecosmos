//! Cosmos UI - A contemplative dual-panel interface
//!
//! Layout:
//! ╔══════════════════════════════════════════════════════════════╗
//! ║                    ☽ C O S M O S ✦                           ║
//! ║          a contemplative companion for your codebase         ║
//! ╠═══════════════════════════╦══════════════════════════════════╣
//! ║  PROJECT                  ║  SUGGESTIONS                     ║
//! ║  ├── src/                 ║  ● Refactor: ai.rs has 715       ║
//! ║  │   ├── main.rs      ●   ║    lines - split into modules    ║
//! ║  │   ├── ui/              ║                                  ║
//! ║  │   └── index/           ║  ◐ Quality: Missing tests for    ║
//! ║  └── tests/               ║    public functions              ║
//! ╠═══════════════════════════╩══════════════════════════════════╣
//! ║  main ● 5 changed │ ? inquiry  ↵ view  a apply  q quit      ║
//! ╚══════════════════════════════════════════════════════════════╝

pub mod panels;
pub mod theme;

use crate::context::WorkContext;
use crate::index::{CodebaseIndex, FileIndex, FlatTreeEntry};
use crate::suggest::{Priority, Suggestion, SuggestionEngine};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::path::PathBuf;
use std::time::Instant;
use theme::Theme;

/// Active panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePanel {
    #[default]
    Project,
    Suggestions,
}

/// Overlay state
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Overlay {
    #[default]
    None,
    Help,
    SuggestionDetail {
        suggestion_id: uuid::Uuid,
        scroll: usize,
    },
    Inquiry {
        response: String,
        scroll: usize,
    },
    ApplyConfirm {
        suggestion_id: uuid::Uuid,
        diff_preview: String,
        scroll: usize,
    },
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

/// Main application state for Cosmos
pub struct App {
    // Core data
    pub index: CodebaseIndex,
    pub suggestions: SuggestionEngine,
    pub context: WorkContext,
    
    // UI state
    pub active_panel: ActivePanel,
    pub project_scroll: usize,
    pub project_selected: usize,
    pub suggestion_scroll: usize,
    pub suggestion_selected: usize,
    pub overlay: Overlay,
    pub toast: Option<Toast>,
    pub should_quit: bool,
    
    // Cached data for display
    pub file_tree: Vec<FlatTreeEntry>,
    pub repo_path: PathBuf,
}

impl App {
    /// Create a new Cosmos app
    pub fn new(
        index: CodebaseIndex,
        suggestions: SuggestionEngine,
        context: WorkContext,
    ) -> Self {
        let file_tree = build_file_tree(&index);
        let repo_path = index.root.clone();
        
        Self {
            index,
            suggestions,
            context,
            active_panel: ActivePanel::default(),
            project_scroll: 0,
            project_selected: 0,
            suggestion_scroll: 0,
            suggestion_selected: 0,
            overlay: Overlay::None,
            toast: None,
            should_quit: false,
            file_tree,
            repo_path,
        }
    }

    /// Switch to the other panel
    pub fn toggle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Project => ActivePanel::Suggestions,
            ActivePanel::Suggestions => ActivePanel::Project,
        };
    }

    /// Navigate down in the current panel
    pub fn navigate_down(&mut self) {
        match self.active_panel {
            ActivePanel::Project => {
                let max = self.file_tree.len().saturating_sub(1);
                self.project_selected = (self.project_selected + 1).min(max);
                self.ensure_project_visible();
            }
            ActivePanel::Suggestions => {
                let max = self.suggestions.active_suggestions().len().saturating_sub(1);
                self.suggestion_selected = (self.suggestion_selected + 1).min(max);
                self.ensure_suggestion_visible();
            }
        }
    }

    /// Navigate up in the current panel
    pub fn navigate_up(&mut self) {
        match self.active_panel {
            ActivePanel::Project => {
                self.project_selected = self.project_selected.saturating_sub(1);
                self.ensure_project_visible();
            }
            ActivePanel::Suggestions => {
                self.suggestion_selected = self.suggestion_selected.saturating_sub(1);
                self.ensure_suggestion_visible();
            }
        }
    }

    fn ensure_project_visible(&mut self) {
        if self.project_selected < self.project_scroll {
            self.project_scroll = self.project_selected;
        } else if self.project_selected >= self.project_scroll + 15 {
            self.project_scroll = self.project_selected.saturating_sub(14);
        }
    }

    fn ensure_suggestion_visible(&mut self) {
        if self.suggestion_selected < self.suggestion_scroll {
            self.suggestion_scroll = self.suggestion_selected;
        } else if self.suggestion_selected >= self.suggestion_scroll + 10 {
            self.suggestion_scroll = self.suggestion_selected.saturating_sub(9);
        }
    }

    /// Get currently selected file
    pub fn selected_file(&self) -> Option<&PathBuf> {
        self.file_tree.get(self.project_selected).map(|e| &e.path)
    }

    /// Get currently selected suggestion
    pub fn selected_suggestion(&self) -> Option<&Suggestion> {
        let suggestions = self.suggestions.active_suggestions();
        suggestions.get(self.suggestion_selected).copied()
    }

    /// Show suggestion detail
    pub fn show_suggestion_detail(&mut self) {
        if let Some(suggestion) = self.selected_suggestion() {
            self.overlay = Overlay::SuggestionDetail {
                suggestion_id: suggestion.id,
                scroll: 0,
            };
        }
    }

    /// Toggle help overlay
    pub fn toggle_help(&mut self) {
        self.overlay = match self.overlay {
            Overlay::Help => Overlay::None,
            _ => Overlay::Help,
        };
    }

    /// Close overlay
    pub fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
    }

    /// Show inquiry response
    pub fn show_inquiry(&mut self, response: String) {
        self.overlay = Overlay::Inquiry { response, scroll: 0 };
    }

    /// Clear expired toast
    pub fn clear_expired_toast(&mut self) {
        if let Some(ref toast) = self.toast {
            if toast.is_expired() {
                self.toast = None;
            }
        }
    }

    /// Show a toast message
    pub fn show_toast(&mut self, message: &str) {
        self.toast = Some(Toast::new(message));
    }

    /// Scroll overlay down
    pub fn overlay_scroll_down(&mut self) {
        match &mut self.overlay {
            Overlay::SuggestionDetail { scroll, .. }
            | Overlay::Inquiry { scroll, .. }
            | Overlay::ApplyConfirm { scroll, .. } => {
                *scroll += 1;
            }
            _ => {}
        }
    }

    /// Scroll overlay up
    pub fn overlay_scroll_up(&mut self) {
        match &mut self.overlay {
            Overlay::SuggestionDetail { scroll, .. }
            | Overlay::Inquiry { scroll, .. }
            | Overlay::ApplyConfirm { scroll, .. } => {
                *scroll = scroll.saturating_sub(1);
            }
            _ => {}
        }
    }

    /// Dismiss the currently selected suggestion
    pub fn dismiss_selected(&mut self) {
        if let Some(suggestion) = self.selected_suggestion() {
            let id = suggestion.id;
            self.suggestions.dismiss(id);
            self.show_toast("Suggestion dismissed");
        }
    }
}

/// Build a flat file tree for display
fn build_file_tree(index: &CodebaseIndex) -> Vec<FlatTreeEntry> {
    let mut entries: Vec<_> = index.files.keys().cloned().collect();
    entries.sort();
    
    entries.into_iter().map(|path| {
        let file_index = index.files.get(&path);
        let priority = file_index.map(|f| f.priority_indicator()).unwrap_or(' ');
        let depth = path.components().count().saturating_sub(1);
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        
        FlatTreeEntry {
            name,
            path,
            is_dir: false,
            depth,
            priority,
        }
    }).collect()
}

// ═══════════════════════════════════════════════════════════════════════════
//  RENDERING
// ═══════════════════════════════════════════════════════════════════════════

/// Main render function
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // Clear with dark background
    frame.render_widget(Block::default().style(Style::default().bg(Theme::BG)), area);

    // Main layout
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),   // Header
            Constraint::Min(10),     // Main content
            Constraint::Length(2),   // Footer
        ])
        .split(area);

    render_header(frame, layout[0], app);
    render_main(frame, layout[1], app);
    render_footer(frame, layout[2], app);

    // Overlays
    match &app.overlay {
        Overlay::Help => render_help(frame),
        Overlay::SuggestionDetail { suggestion_id, scroll } => {
            if let Some(suggestion) = app.suggestions.suggestions.iter().find(|s| &s.id == suggestion_id) {
                render_suggestion_detail(frame, suggestion, *scroll);
            }
        }
        Overlay::Inquiry { response, scroll } => {
            render_inquiry(frame, response, *scroll);
        }
        Overlay::ApplyConfirm { diff_preview, scroll, .. } => {
            render_apply_confirm(frame, diff_preview, *scroll);
        }
        Overlay::None => {}
    }

    // Toast
    if let Some(toast) = &app.toast {
        render_toast(frame, toast);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("   {} ", Theme::COSMOS_HEADER),
                Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD)
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("   {}", Theme::COSMOS_TAGLINE),
                Style::default().fg(Theme::GREY_500).add_modifier(Modifier::ITALIC)
            ),
        ]),
    ];

    let header = Paragraph::new(lines).style(Style::default().bg(Theme::BG));
    frame.render_widget(header, area);
}

fn render_main(frame: &mut Frame, area: Rect, app: &App) {
    // Split into two panels
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),  // Project tree
            Constraint::Percentage(60),  // Suggestions
        ])
        .split(area);

    render_project_panel(frame, panels[0], app);
    render_suggestions_panel(frame, panels[1], app);
}

fn render_project_panel(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_panel == ActivePanel::Project;
    let border_style = if is_active {
        Style::default().fg(Theme::GREY_300)
    } else {
        Style::default().fg(Theme::GREY_600)
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    
    let mut lines = vec![];
    
    for (i, entry) in app.file_tree.iter()
        .enumerate()
        .skip(app.project_scroll)
        .take(visible_height)
    {
        let is_selected = i == app.project_selected && is_active;
        let indent = "  ".repeat(entry.depth);
        let prefix = if entry.is_dir {
            Theme::TREE_FOLDER_OPEN.to_string()
        } else {
            Theme::TREE_FILE.to_string()
        };
        
        let name_style = if is_selected {
            Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD)
        } else if entry.priority == Theme::PRIORITY_HIGH {
            Style::default().fg(Theme::GREY_100)
        } else {
            Style::default().fg(Theme::GREY_400)
        };
        
        let cursor = if is_selected { "›" } else { " " };
        let priority_indicator = if entry.priority != ' ' {
            format!(" {}", entry.priority)
        } else {
            "  ".to_string()
        };
        
        lines.push(Line::from(vec![
            Span::styled(cursor, Style::default().fg(Theme::WHITE)),
            Span::styled(format!(" {}{} ", indent, prefix), Style::default().fg(Theme::GREY_600)),
            Span::styled(&entry.name, name_style),
            Span::styled(priority_indicator, Style::default().fg(Theme::GREY_500)),
        ]));
    }

    let block = Block::default()
        .title(format!(" {} ", Theme::SECTION_PROJECT))
        .title_style(Style::default().fg(Theme::GREY_300))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(Theme::GREY_800));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_suggestions_panel(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_panel == ActivePanel::Suggestions;
    let border_style = if is_active {
        Style::default().fg(Theme::GREY_300)
    } else {
        Style::default().fg(Theme::GREY_600)
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    let suggestions = app.suggestions.active_suggestions();
    
    let mut lines = vec![];
    
    if suggestions.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                "  No suggestions - your code looks contemplative ",
                Style::default().fg(Theme::GREY_500).add_modifier(Modifier::ITALIC)
            ),
        ]));
    } else {
        for (i, suggestion) in suggestions.iter()
            .enumerate()
            .skip(app.suggestion_scroll)
            .take(visible_height)
        {
            let is_selected = i == app.suggestion_selected && is_active;
            
            let priority_style = match suggestion.priority {
                Priority::High => Style::default().fg(Theme::WHITE),
                Priority::Medium => Style::default().fg(Theme::GREY_300),
                Priority::Low => Style::default().fg(Theme::GREY_500),
            };
            
            let text_style = if is_selected {
                Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Theme::GREY_200)
            };
            
            let cursor = if is_selected { "›" } else { " " };
            let file_name = suggestion.file.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            
            lines.push(Line::from(vec![
                Span::styled(cursor, Style::default().fg(Theme::WHITE)),
                Span::styled(format!(" {} ", suggestion.priority.icon()), priority_style),
                Span::styled(format!("{}: ", suggestion.kind.label()), Style::default().fg(Theme::GREY_400)),
                Span::styled(truncate(&suggestion.summary, 40), text_style),
            ]));
            
            // Second line: file info
            lines.push(Line::from(vec![
                Span::styled("     ", Style::default()),
                Span::styled(format!("in {}", file_name), Style::default().fg(Theme::GREY_600)),
            ]));
        }
    }

    let counts = app.suggestions.counts();
    let title = format!(
        " {} ({} {} {} {}) ",
        Theme::SECTION_SUGGESTIONS,
        counts.high, Theme::PRIORITY_HIGH,
        counts.medium, Theme::PRIORITY_MED,
    );

    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(Theme::GREY_300))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(Theme::GREY_800));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![
        Span::styled(" ", Style::default()),
        Span::styled(&app.context.branch, Style::default().fg(Theme::GREY_400)),
    ];

    if app.context.has_changes() {
        spans.push(Span::styled(" │ ", Style::default().fg(Theme::GREY_600)));
        spans.push(Span::styled(
            format!("{} changed", app.context.modified_count),
            Style::default().fg(Theme::GREY_300),
        ));
    }

    spans.push(Span::styled(" │ ", Style::default().fg(Theme::GREY_600)));
    
    // Key hints
    let hints = [
        ("?", "help"),
        ("Tab", "switch"),
        ("↵", "view"),
        ("a", "apply"),
        ("d", "dismiss"),
        ("q", "quit"),
    ];
    
    for (key, action) in hints {
        spans.push(Span::styled(key, Style::default().fg(Theme::GREY_300)));
        spans.push(Span::styled(format!(" {} ", action), Style::default().fg(Theme::GREY_600)));
    }

    let footer = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Theme::GREY_800));
    frame.render_widget(footer, area);
}

// ═══════════════════════════════════════════════════════════════════════════
//  OVERLAYS
// ═══════════════════════════════════════════════════════════════════════════

fn render_help(frame: &mut Frame) {
    let area = centered_rect(50, 70, frame.area());
    frame.render_widget(Clear, area);

    let help_text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  NAVIGATION", Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD))
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ↑/k ↓/j   ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Navigate", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(vec![
            Span::styled("  Tab       ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Switch panels", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(vec![
            Span::styled("  Enter     ", Style::default().fg(Theme::GREY_300)),
            Span::styled("View details", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ACTIONS", Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD))
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ?         ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Inquiry - ask for suggestions", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(vec![
            Span::styled("  a         ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Apply suggestion", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(vec![
            Span::styled("  d         ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Dismiss suggestion", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(vec![
            Span::styled("  r         ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Refresh index", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Esc       ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Close / Back", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(vec![
            Span::styled("  q         ", Style::default().fg(Theme::GREY_300)),
            Span::styled("Quit", Style::default().fg(Theme::GREY_500)),
        ]),
        Line::from(""),
    ];

    let block = Paragraph::new(help_text)
        .block(Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::GREY_500))
            .style(Style::default().bg(Theme::GREY_800)));
    
    frame.render_widget(block, area);
}

fn render_suggestion_detail(frame: &mut Frame, suggestion: &Suggestion, scroll: usize) {
    let area = centered_rect(70, 75, frame.area());
    frame.render_widget(Clear, area);

    let visible_height = area.height.saturating_sub(8) as usize;
    
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  {} ", suggestion.priority.icon()), 
                Style::default().fg(Theme::WHITE)),
            Span::styled(suggestion.kind.label(), 
                Style::default().fg(Theme::GREY_300)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  {}", suggestion.summary), 
                Style::default().fg(Theme::GREY_100)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  File: {}", suggestion.file.display()), 
                Style::default().fg(Theme::GREY_400)),
        ]),
    ];

    if let Some(line) = suggestion.line {
        lines.push(Line::from(vec![
            Span::styled(format!("  Line: {}", line), 
                Style::default().fg(Theme::GREY_400)),
        ]));
    }

    lines.push(Line::from(""));

    if let Some(detail) = &suggestion.detail {
        lines.push(Line::from(vec![
            Span::styled("  DETAILS", Style::default().fg(Theme::GREY_300).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(""));
        
        for line in detail.lines().skip(scroll).take(visible_height) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", line), Style::default().fg(Theme::GREY_200)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  a", Style::default().fg(Theme::GREY_300)),
        Span::styled(" apply  ", Style::default().fg(Theme::GREY_500)),
        Span::styled("d", Style::default().fg(Theme::GREY_300)),
        Span::styled(" dismiss  ", Style::default().fg(Theme::GREY_500)),
        Span::styled("Esc", Style::default().fg(Theme::GREY_300)),
        Span::styled(" close", Style::default().fg(Theme::GREY_500)),
    ]));

    let block = Paragraph::new(lines)
        .block(Block::default()
            .title(" Suggestion ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::WHITE))
            .style(Style::default().bg(Theme::GREY_800)));
    
    frame.render_widget(block, area);
}

fn render_inquiry(frame: &mut Frame, response: &str, scroll: usize) {
    let area = centered_rect(75, 80, frame.area());
    frame.render_widget(Clear, area);

    let visible_height = area.height.saturating_sub(6) as usize;
    
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ✦ ", Style::default().fg(Theme::WHITE)),
            Span::styled("Cosmos suggests...", Style::default().fg(Theme::GREY_200).add_modifier(Modifier::ITALIC)),
        ]),
        Line::from(""),
    ];

    for line in response.lines().skip(scroll).take(visible_height) {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", line), Style::default().fg(Theme::GREY_200)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  ↑↓ scroll  Esc close", Style::default().fg(Theme::GREY_500)),
    ]));

    let block = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .title(" Inquiry ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::WHITE))
            .style(Style::default().bg(Theme::GREY_800)));
    
    frame.render_widget(block, area);
}

fn render_apply_confirm(frame: &mut Frame, diff_preview: &str, scroll: usize) {
    let area = centered_rect(80, 85, frame.area());
    frame.render_widget(Clear, area);

    let visible_height = area.height.saturating_sub(8) as usize;
    
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Apply these changes?", Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
    ];

    for line in diff_preview.lines().skip(scroll).take(visible_height) {
        let style = if line.starts_with('+') {
            Style::default().fg(Theme::GREEN)
        } else if line.starts_with('-') {
            Style::default().fg(Theme::RED)
        } else if line.starts_with("@@") {
            Style::default().fg(Theme::GREY_400)
        } else {
            Style::default().fg(Theme::GREY_300)
        };
        
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", line), style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  y", Style::default().fg(Theme::GREEN)),
        Span::styled(" apply  ", Style::default().fg(Theme::GREY_500)),
        Span::styled("n", Style::default().fg(Theme::RED)),
        Span::styled(" cancel", Style::default().fg(Theme::GREY_500)),
    ]));

    let block = Paragraph::new(lines)
        .block(Block::default()
            .title(" Confirm ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::WHITE))
            .style(Style::default().bg(Theme::GREY_800)));
    
    frame.render_widget(block, area);
}

fn render_toast(frame: &mut Frame, toast: &Toast) {
    let area = frame.area();
    let width = (toast.message.len() + 6) as u16;
    let toast_area = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: area.height.saturating_sub(4),
        width: width.min(area.width),
        height: 1,
    };

    let content = Paragraph::new(Line::from(vec![
        Span::styled(" ✓ ", Style::default().fg(Theme::WHITE)),
        Span::styled(&toast.message, Style::default().fg(Theme::GREY_200)),
        Span::styled(" ", Style::default()),
    ]))
    .style(Style::default().bg(Theme::GREY_700));

    frame.render_widget(content, toast_area);
}

// ═══════════════════════════════════════════════════════════════════════════
//  UTILITIES
// ═══════════════════════════════════════════════════════════════════════════

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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
