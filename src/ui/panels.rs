//! Panel utilities for Cosmos UI
//!
//! Helper functions for rendering panels and list items.

use crate::ui::theme::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// Format a time duration in days as a human-readable string
pub fn format_time_ago(days: i64) -> String {
    if days == 0 {
        "today".to_string()
    } else if days == 1 {
        "1d".to_string()
    } else if days < 7 {
        format!("{}d", days)
    } else if days < 30 {
        format!("{}w", days / 7)
    } else if days < 365 {
        format!("{}mo", days / 30)
    } else {
        format!("{}y", days / 365)
    }
}

/// Truncate a file path for display
pub fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        let start = path.len() - max_len + 3;
        format!("...{}", &path[start..])
    }
}

/// Truncate text for display
pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}

/// Create a progress bar string
pub fn progress_bar(value: f64, width: usize) -> String {
    let filled = ((value.clamp(0.0, 1.0)) * width as f64) as usize;
    let mut bar = String::new();
    
    for i in 0..width {
        if i < filled {
            bar.push(Theme::BAR_FILLED);
        } else {
            bar.push(Theme::BAR_EMPTY);
        }
    }
    
    bar
}

/// Create a styled line for a file tree entry
pub fn tree_line(
    name: &str,
    depth: usize,
    is_dir: bool,
    is_selected: bool,
    has_suggestions: bool,
) -> Line<'static> {
    let indent = "  ".repeat(depth);
    let icon = if is_dir {
        Theme::TREE_FOLDER_OPEN
    } else {
        Theme::TREE_FILE
    };
    
    let style = if is_selected {
        Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD)
    } else if has_suggestions {
        Style::default().fg(Theme::GREY_100)
    } else if is_dir {
        Style::default().fg(Theme::GREY_300)
    } else {
        Style::default().fg(Theme::GREY_400)
    };
    
    let cursor = if is_selected { "› " } else { "  " };
    
    Line::from(vec![
        Span::styled(cursor, Style::default().fg(Theme::WHITE)),
        Span::styled(format!("{}{} ", indent, icon), Style::default().fg(Theme::GREY_600)),
        Span::styled(name.to_string(), style),
    ])
}

/// Create a styled suggestion line
pub fn suggestion_line(
    priority_icon: char,
    kind_label: &str,
    summary: &str,
    is_selected: bool,
) -> Line<'static> {
    let priority_style = match priority_icon {
        '\u{25CF}' => Style::default().fg(Theme::WHITE),  // High
        '\u{25D0}' => Style::default().fg(Theme::GREY_300),  // Medium
        _ => Style::default().fg(Theme::GREY_500),  // Low
    };
    
    let text_style = if is_selected {
        Style::default().fg(Theme::WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Theme::GREY_200)
    };
    
    let cursor = if is_selected { "› " } else { "  " };
    
    Line::from(vec![
        Span::styled(cursor, Style::default().fg(Theme::WHITE)),
        Span::styled(format!("{} ", priority_icon), priority_style),
        Span::styled(format!("{}: ", kind_label), Style::default().fg(Theme::GREY_400)),
        Span::styled(truncate_text(summary, 45), text_style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time_ago() {
        assert_eq!(format_time_ago(0), "today");
        assert_eq!(format_time_ago(1), "1d");
        assert_eq!(format_time_ago(7), "1w");
        assert_eq!(format_time_ago(30), "1mo");
        assert_eq!(format_time_ago(365), "1y");
    }

    #[test]
    fn test_truncate_path() {
        let path = "src/very/long/path/to/file.rs";
        let truncated = truncate_path(path, 20);
        assert!(truncated.starts_with("..."));
        assert!(truncated.len() <= 20);
    }

    #[test]
    fn test_progress_bar() {
        let bar = progress_bar(0.5, 10);
        assert_eq!(bar.chars().count(), 10);
    }
}
