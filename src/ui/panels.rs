//! Panel rendering for each data view

use crate::analysis::{
    AuthorStats, BusFactorRisk, ChurnEntry, DangerZone, DustyFile, TestCoverage, TestSummary,
    TodoEntry, TodoKind,
};
use crate::ui::theme::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};

pub struct Panel;

impl Panel {
    /// Create the danger zones panel showing high-churn + high-complexity files
    pub fn danger_zones<'a>(
        zones: &'a [DangerZone],
        scroll_offset: usize,
        selected: Option<usize>,
        search: &'a str,
    ) -> impl Widget + 'a {
        let filtered: Vec<_> = if search.is_empty() {
            zones.iter().collect()
        } else {
            let q = search.to_lowercase();
            zones.iter().filter(|z| z.path.to_lowercase().contains(&q)).collect()
        };

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .flat_map(|(idx, zone)| {
                let is_selected = selected == Some(idx);

                // Risk indicator based on danger score
                let (risk_indicator, risk_style) = if zone.danger_score >= 70.0 {
                    (Theme::RISK_CRITICAL, Theme::danger_critical())
                } else if zone.danger_score >= 50.0 {
                    (Theme::RISK_HIGH, Theme::danger_high())
                } else {
                    (Theme::RISK_MEDIUM, Theme::danger_medium())
                };

                let base_style = if is_selected {
                    Theme::selected()
                } else {
                    Theme::text()
                };

                // Main line with file path
                let main_line = Line::from(vec![
                    Span::styled(format!(" {} ", risk_indicator), risk_style),
                    Span::styled(truncate_path(&zone.path, 50), base_style),
                ]);

                // Detail line with stats
                let detail_line = Line::from(vec![
                    Span::raw("      "),
                    Span::styled(format!("{}×", zone.change_count), Theme::text_muted()),
                    Span::styled(" │ ", Theme::text_dim()),
                    Span::styled(format!("c:{:.1}", zone.complexity_score), Theme::text_muted()),
                    Span::styled(" │ ", Theme::text_dim()),
                    Span::styled(&zone.reason, Theme::text_dim()),
                ]);

                vec![ListItem::new(main_line), ListItem::new(detail_line)]
            })
            .collect();

        let title = format!(
            " {} danger zones {} ",
            Theme::DIAMOND_FILLED,
            if !search.is_empty() {
                format!("(filtered: {})", filtered.len())
            } else {
                format!("({})", zones.len())
            }
        );

        List::new(items).block(
            Block::default()
                .title(Span::styled(title, Theme::title()))
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .style(Theme::panel_bg()),
        )
    }

    /// Create the hotspots panel showing files with highest churn
    pub fn hotspots<'a>(
        entries: &'a [ChurnEntry],
        scroll_offset: usize,
        selected: Option<usize>,
        search: &'a str,
    ) -> impl Widget + 'a {
        let filtered: Vec<_> = if search.is_empty() {
            entries.iter().collect()
        } else {
            let q = search.to_lowercase();
            entries.iter().filter(|e| e.path.to_lowercase().contains(&q)).collect()
        };

        let max_changes = filtered.first().map(|e| e.change_count).unwrap_or(1);

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .map(|(idx, entry)| {
                let is_selected = selected == Some(idx);

                // Visual bar showing relative churn
                let bar_width = 8;
                let filled = (entry.change_count * bar_width) / max_changes.max(1);
                let bar: String = (0..bar_width)
                    .map(|i| if i < filled { Theme::BAR_FILLED } else { Theme::BAR_EMPTY })
                    .collect();

                let base_style = if is_selected {
                    Theme::selected()
                } else {
                    Theme::text()
                };

                let line = Line::from(vec![
                    Span::styled(" ", Style::default()),
                    Span::styled(bar, Theme::text_muted()),
                    Span::styled(" ", Style::default()),
                    Span::styled(format!("{:3}×", entry.change_count), Theme::text_muted()),
                    Span::styled(" ", Style::default()),
                    Span::styled(truncate_path(&entry.path, 45), base_style),
                    Span::styled(
                        format!("  {}", format_time_ago(entry.days_active)),
                        Theme::text_dim(),
                    ),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = format!(
            " {} hotspots {} ",
            Theme::BULLET_FILLED,
            if !search.is_empty() {
                format!("(filtered: {})", filtered.len())
            } else {
                format!("({})", entries.len())
            }
        );

        List::new(items).block(
            Block::default()
                .title(Span::styled(title, Theme::title()))
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .style(Theme::panel_bg()),
        )
    }

    /// Create the dusty files panel showing old untouched files
    pub fn dusty_files<'a>(
        files: &'a [DustyFile],
        scroll_offset: usize,
        selected: Option<usize>,
        search: &'a str,
    ) -> impl Widget + 'a {
        let filtered: Vec<_> = if search.is_empty() {
            files.iter().collect()
        } else {
            let q = search.to_lowercase();
            files.iter().filter(|f| f.path.to_lowercase().contains(&q)).collect()
        };

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .map(|(idx, file)| {
                let is_selected = selected == Some(idx);

                // Staleness indicator (more dots = older)
                let staleness_dots = match file.days_since_change {
                    0..=120 => "·",
                    121..=240 => "··",
                    241..=365 => "···",
                    _ => "····",
                };

                let base_style = if is_selected {
                    Theme::selected()
                } else {
                    Theme::text()
                };

                let line = Line::from(vec![
                    Span::styled(format!(" {:4} ", staleness_dots), Theme::text_dim()),
                    Span::styled(truncate_path(&file.path, 45), base_style),
                    Span::styled(
                        format!("  {} lines", file.line_count),
                        Theme::text_dim(),
                    ),
                    Span::styled(
                        format!("  {}", format_time_ago(file.days_since_change)),
                        Theme::text_muted(),
                    ),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = format!(
            " {} dusty files {} ",
            Theme::BULLET_EMPTY,
            if !search.is_empty() {
                format!("(filtered: {})", filtered.len())
            } else {
                format!("({})", files.len())
            }
        );

        List::new(items).block(
            Block::default()
                .title(Span::styled(title, Theme::title()))
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .style(Theme::panel_bg()),
        )
    }

    /// Create the TODOs panel showing all TODO/HACK/FIXME entries
    pub fn todos<'a>(
        entries: &'a [TodoEntry],
        scroll_offset: usize,
        selected: Option<usize>,
        search: &'a str,
    ) -> impl Widget + 'a {
        let filtered: Vec<_> = if search.is_empty() {
            entries.iter().collect()
        } else {
            let q = search.to_lowercase();
            entries
                .iter()
                .filter(|e| e.path.to_lowercase().contains(&q) || e.text.to_lowercase().contains(&q))
                .collect()
        };

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .map(|(idx, entry)| {
                let is_selected = selected == Some(idx);

                // Kind indicator with different intensity
                let kind_style = match entry.kind {
                    TodoKind::Fixme => Theme::danger_critical(),
                    TodoKind::Hack => Theme::danger_high(),
                    TodoKind::Todo => Theme::text(),
                    TodoKind::Xxx => Theme::danger_medium(),
                };

                let base_style = if is_selected {
                    Theme::selected()
                } else {
                    Theme::text()
                };

                let line = Line::from(vec![
                    Span::styled(format!(" {:6} ", entry.kind.as_str()), kind_style),
                    Span::styled(
                        format!("{}:{}", truncate_path(&entry.path, 25), entry.line_number),
                        Theme::text_muted(),
                    ),
                    Span::raw("  "),
                    Span::styled(truncate_text(&entry.text, 40), base_style),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = format!(
            " {} todos & hacks {} ",
            Theme::ARROW_RIGHT,
            if !search.is_empty() {
                format!("(filtered: {})", filtered.len())
            } else {
                format!("({})", entries.len())
            }
        );

        List::new(items).block(
            Block::default()
                .title(Span::styled(title, Theme::title()))
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .style(Theme::panel_bg()),
        )
    }

    /// Create the bus factor panel showing single-author risk files
    pub fn bus_factor<'a>(
        risks: &'a [BusFactorRisk],
        stats: Option<&'a AuthorStats>,
        scroll_offset: usize,
        selected: Option<usize>,
        search: &'a str,
    ) -> impl Widget + 'a {
        let filtered: Vec<_> = if search.is_empty() {
            risks.iter().collect()
        } else {
            let q = search.to_lowercase();
            risks
                .iter()
                .filter(|r| r.path.to_lowercase().contains(&q) || r.primary_author.to_lowercase().contains(&q))
                .collect()
        };

        let mut items: Vec<ListItem> = Vec::new();

        // Add summary header if we have stats
        if let Some(s) = stats {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!(
                        " {} authors │ avg bus factor {:.1} │ {} single-author files",
                        s.total_authors, s.avg_bus_factor, s.single_author_files
                    ),
                    Theme::text_dim(),
                ),
            ])));
            items.push(ListItem::new(Line::from("")));
        }

        // Add risk entries
        for (idx, risk) in filtered.iter().enumerate().skip(scroll_offset) {
            let is_selected = selected == Some(idx);

            // Risk level indicator
            let risk_indicator = if risk.primary_author_pct >= 95.0 {
                Theme::RISK_CRITICAL
            } else if risk.primary_author_pct >= 80.0 {
                Theme::RISK_HIGH
            } else {
                Theme::RISK_MEDIUM
            };

            let base_style = if is_selected {
                Theme::selected()
            } else {
                Theme::text()
            };

            let main_line = Line::from(vec![
                Span::styled(format!(" {} ", risk_indicator), Theme::text_muted()),
                Span::styled(truncate_path(&risk.path, 40), base_style),
            ]);

            let detail_line = Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    truncate_text(&risk.primary_author, 20),
                    Theme::text_muted(),
                ),
                Span::styled(
                    format!(" ({:.0}%)", risk.primary_author_pct),
                    Theme::text_dim(),
                ),
                Span::styled(" │ ", Theme::text_dim()),
                Span::styled(&risk.risk_reason, Theme::text_dim()),
            ]);

            items.push(ListItem::new(main_line));
            items.push(ListItem::new(detail_line));
        }

        if filtered.is_empty() && risks.is_empty() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    "  No bus factor risks detected. Code ownership is well distributed.",
                    Theme::text_dim(),
                ),
            ])));
        }

        let title = format!(
            " {} bus factor {} ",
            Theme::BULLET_HALF,
            if !search.is_empty() {
                format!("(filtered: {})", filtered.len())
            } else {
                format!("({})", risks.len())
            }
        );

        List::new(items).block(
            Block::default()
                .title(Span::styled(title, Theme::title()))
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .style(Theme::panel_bg()),
        )
    }

    /// Create the tests panel showing test coverage status
    pub fn tests<'a>(
        coverages: &'a [TestCoverage],
        summary: Option<&'a TestSummary>,
        scroll_offset: usize,
        selected: Option<usize>,
        search: &'a str,
    ) -> impl Widget + 'a {
        let filtered: Vec<_> = if search.is_empty() {
            coverages.iter().collect()
        } else {
            let q = search.to_lowercase();
            coverages.iter().filter(|c| c.path.to_lowercase().contains(&q)).collect()
        };

        let mut items: Vec<ListItem> = Vec::new();

        // Add summary header if we have it
        if let Some(s) = summary {
            let coverage_bar_width = 20;
            let filled = (s.coverage_pct as usize * coverage_bar_width) / 100;
            let bar: String = (0..coverage_bar_width)
                .map(|i| if i < filled { Theme::BAR_FILLED } else { Theme::BAR_EMPTY })
                .collect();

            items.push(ListItem::new(Line::from(vec![
                Span::styled(" coverage ", Theme::text_dim()),
                Span::styled(bar, Theme::text_muted()),
                Span::styled(format!(" {:.0}%", s.coverage_pct), Theme::text()),
            ])));

            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!(
                        " {} tested │ {} untested │ {} danger zones untested",
                        s.files_with_tests, s.files_without_tests, s.untested_danger_zones.len()
                    ),
                    Theme::text_dim(),
                ),
            ])));

            items.push(ListItem::new(Line::from("")));
        }

        // Add untested files first (they're already sorted this way)
        for (idx, coverage) in filtered.iter().enumerate().skip(scroll_offset) {
            let is_selected = selected == Some(idx);

            let (indicator, indicator_style) = if coverage.has_tests {
                if coverage.inline_tests {
                    (Theme::BULLET_HALF, Theme::text_muted())
                } else {
                    (Theme::BULLET_FILLED, Theme::text())
                }
            } else {
                (Theme::BULLET_EMPTY, Theme::danger_high())
            };

            let base_style = if is_selected {
                Theme::selected()
            } else if coverage.has_tests {
                Theme::text()
            } else {
                Style::default()
                    .fg(Theme::GREY_200)
                    .add_modifier(Modifier::BOLD)
            };

            let status_text = if coverage.has_tests {
                if coverage.inline_tests {
                    "inline"
                } else {
                    "tested"
                }
            } else {
                "NO TESTS"
            };

            let line = Line::from(vec![
                Span::styled(format!(" {} ", indicator), indicator_style),
                Span::styled(truncate_path(&coverage.path, 45), base_style),
                Span::styled(
                    format!("  {} lines", coverage.source_line_count),
                    Theme::text_dim(),
                ),
                Span::styled(format!("  {}", status_text), Theme::text_muted()),
            ]);

            items.push(ListItem::new(line));
        }

        let untested_count = coverages.iter().filter(|c| !c.has_tests).count();
        let title = format!(
            " {} test coverage {} ",
            Theme::DIAMOND_EMPTY,
            if !search.is_empty() {
                format!("(filtered: {})", filtered.len())
            } else {
                format!("({} untested)", untested_count)
            }
        );

        List::new(items).block(
            Block::default()
                .title(Span::styled(title, Theme::title()))
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .style(Theme::panel_bg()),
        )
    }
}

fn format_time_ago(days: i64) -> String {
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

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        let start = path.len() - max_len + 3;
        format!("...{}", &path[start..])
    }
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}
