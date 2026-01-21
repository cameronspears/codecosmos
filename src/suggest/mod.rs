//! Suggestion engine for Cosmos
//!
//! Tiered approach to minimize LLM spend:
//! - Layer 1: Static rules (FREE)
//! - Layer 2: Cached suggestions (ONE-TIME)
//! - Layer 3: Grok Fast for categorization (~$0.0001/call)
//! - Layer 4: LLM for deep analysis (Speed for analysis, Smart for code gen)

pub mod llm;

use crate::index::{CodebaseIndex, Pattern, PatternKind, PatternSeverity};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Source of a suggestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionSource {
    /// Pattern matching, no LLM cost
    Static,
    /// Previously generated, loaded from cache
    Cached,
    /// Grok Fast for quick categorization
    LlmFast,
    /// LLM for detailed analysis
    LlmDeep,
}

impl SuggestionSource {
    pub fn icon(&self) -> &'static str {
        match self {
            SuggestionSource::Static => "  ",
            SuggestionSource::Cached => " ",
            SuggestionSource::LlmFast => " ",
            SuggestionSource::LlmDeep => " ",
        }
    }
}

/// Kind of suggestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionKind {
    /// Code improvement/refactoring
    Improvement,
    /// Potential bug fix
    BugFix,
    /// New feature suggestion
    Feature,
    /// Performance optimization
    Optimization,
    /// Code quality/maintainability
    Quality,
    /// Documentation improvement
    Documentation,
    /// Test coverage
    Testing,
    /// Code refactoring (extract, rename, restructure)
    Refactoring,
}

impl SuggestionKind {
    pub fn icon(&self) -> char {
        match self {
            SuggestionKind::Improvement => '\u{2728}',  // ✨
            SuggestionKind::BugFix => '\u{1F41B}',      // 🐛
            SuggestionKind::Feature => '\u{2795}',      // ➕
            SuggestionKind::Optimization => '\u{26A1}', // ⚡
            SuggestionKind::Quality => '\u{2726}',      // ✦
            SuggestionKind::Documentation => '\u{1F4DD}', // 📝
            SuggestionKind::Testing => '\u{1F9EA}',     // 🧪
            SuggestionKind::Refactoring => '\u{1F527}', // 🔧
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SuggestionKind::Improvement => "Improve",
            SuggestionKind::BugFix => "Fix",
            SuggestionKind::Feature => "Feature",
            SuggestionKind::Optimization => "Optimize",
            SuggestionKind::Quality => "Quality",
            SuggestionKind::Documentation => "Docs",
            SuggestionKind::Testing => "Test",
            SuggestionKind::Refactoring => "Refactor",
        }
    }
}

/// Priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn icon(&self) -> char {
        match self {
            Priority::High => '\u{25CF}',   // 
            Priority::Medium => '\u{25D0}', // 
            Priority::Low => '\u{25CB}',    // 
        }
    }

    pub fn from_severity(severity: PatternSeverity) -> Self {
        match severity {
            PatternSeverity::High => Priority::High,
            PatternSeverity::Medium => Priority::Medium,
            PatternSeverity::Low | PatternSeverity::Info => Priority::Low,
        }
    }
}

/// A suggestion for improvement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: Uuid,
    pub kind: SuggestionKind,
    pub priority: Priority,
    /// Primary file (used for display/grouping)
    pub file: PathBuf,
    /// Additional files affected by this suggestion (for multi-file refactors)
    #[serde(default)]
    pub additional_files: Vec<PathBuf>,
    pub line: Option<usize>,
    pub summary: String,
    pub detail: Option<String>,
    pub source: SuggestionSource,
    pub created_at: DateTime<Utc>,
    /// Whether the user has dismissed this suggestion
    pub dismissed: bool,
    /// Whether the suggestion has been applied
    pub applied: bool,
}

impl Suggestion {
    pub fn new(
        kind: SuggestionKind,
        priority: Priority,
        file: PathBuf,
        summary: String,
        source: SuggestionSource,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            priority,
            file,
            additional_files: Vec::new(),
            line: None,
            summary,
            detail: None,
            source,
            created_at: Utc::now(),
            dismissed: false,
            applied: false,
        }
    }

    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    pub fn with_detail(mut self, detail: String) -> Self {
        self.detail = Some(detail);
        self
    }

    pub fn with_additional_files(mut self, files: Vec<PathBuf>) -> Self {
        self.additional_files = files;
        self
    }

    /// Get all files affected by this suggestion (primary + additional)
    pub fn affected_files(&self) -> Vec<&PathBuf> {
        std::iter::once(&self.file)
            .chain(self.additional_files.iter())
            .collect()
    }

    /// Check if this is a multi-file suggestion
    pub fn is_multi_file(&self) -> bool {
        !self.additional_files.is_empty()
    }

    /// Get the total number of files affected
    pub fn file_count(&self) -> usize {
        1 + self.additional_files.len()
    }

    /// Format for display in the suggestion list
    pub fn display_summary(&self) -> String {
        let file_indicator = if self.is_multi_file() {
            format!(" [{}]", self.file_count())
        } else {
            String::new()
        };
        
        if let Some(line) = self.line {
            format!("{}:{}{} - {}", self.file.display(), line, file_indicator, self.summary)
        } else {
            format!("{}{} - {}", self.file.display(), file_indicator, self.summary)
        }
    }
}

/// The suggestion engine
pub struct SuggestionEngine {
    pub suggestions: Vec<Suggestion>,
    pub index: CodebaseIndex,
}

impl SuggestionEngine {
    /// Create a new suggestion engine from a codebase index
    /// 
    /// Starts empty - LLM suggestions are generated separately.
    pub fn new(index: CodebaseIndex) -> Self {
        Self {
            suggestions: Vec::new(),
            index,
        }
    }

    /// Get all active suggestions (not dismissed/applied)
    pub fn active_suggestions(&self) -> Vec<&Suggestion> {
        self.suggestions
            .iter()
            .filter(|s| !s.dismissed && !s.applied)
            .collect()
    }

    /// Get high priority suggestions
    pub fn high_priority_suggestions(&self) -> Vec<&Suggestion> {
        self.active_suggestions()
            .into_iter()
            .filter(|s| s.priority == Priority::High)
            .collect()
    }

    /// Dismiss a suggestion
    #[allow(dead_code)]
    pub fn dismiss(&mut self, id: Uuid) {
        if let Some(s) = self.suggestions.iter_mut().find(|s| s.id == id) {
            s.dismissed = true;
        }
    }

    /// Mark a suggestion as applied
    pub fn mark_applied(&mut self, id: Uuid) {
        if let Some(s) = self.suggestions.iter_mut().find(|s| s.id == id) {
            s.applied = true;
        }
    }

    /// Mark a suggestion as not applied (used for undo).
    pub fn unmark_applied(&mut self, id: Uuid) {
        if let Some(s) = self.suggestions.iter_mut().find(|s| s.id == id) {
            s.applied = false;
        }
    }

    /// Generate static refactoring suggestions from detected patterns.
    /// 
    /// This provides immediate, free suggestions based on code patterns
    /// detected during indexing (long functions, deep nesting, etc.).
    pub fn generate_static_suggestions(&mut self) {
        for pattern in &self.index.patterns {
            if let Some(suggestion) = pattern_to_refactoring_suggestion(pattern) {
                self.suggestions.push(suggestion);
            }
        }
        // Sort by priority after adding static suggestions
        self.suggestions.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Add a suggestion from LLM
    pub fn add_llm_suggestion(&mut self, suggestion: Suggestion) {
        self.suggestions.push(suggestion);
        self.suggestions.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Sort suggestions with git context: changed files first, then blast radius, then priority.
    pub fn sort_with_context(&mut self, context: &crate::context::WorkContext) {
        let changed: std::collections::HashSet<PathBuf> = context
            .all_changed_files()
            .into_iter()
            .cloned()
            .collect();

        // “Blast radius” = files that import changed files (and direct deps of changed files).
        let mut blast: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for path in &changed {
            if let Some(file_index) = self.index.files.get(path) {
                for u in &file_index.summary.used_by {
                    blast.insert(u.clone());
                }
                for d in &file_index.summary.depends_on {
                    blast.insert(d.clone());
                }
            }
        }
        for c in &changed {
            blast.remove(c);
        }

        let kind_weight = |k: SuggestionKind| -> i64 {
            match k {
                SuggestionKind::BugFix => 40,
                SuggestionKind::Refactoring => 30,
                SuggestionKind::Optimization => 25,
                SuggestionKind::Testing => 20,
                SuggestionKind::Quality => 15,
                SuggestionKind::Documentation => 10,
                SuggestionKind::Improvement => 10,
                SuggestionKind::Feature => 0,
            }
        };

        self.suggestions.sort_by(|a, b| {
            let a_changed = changed.contains(&a.file);
            let b_changed = changed.contains(&b.file);
            if a_changed != b_changed {
                return b_changed.cmp(&a_changed);
            }

            let a_blast = blast.contains(&a.file);
            let b_blast = blast.contains(&b.file);
            if a_blast != b_blast {
                return b_blast.cmp(&a_blast);
            }

            // Higher priority first
            let pri = b.priority.cmp(&a.priority);
            if pri != std::cmp::Ordering::Equal {
                return pri;
            }

            // Then kind weight
            let kw = kind_weight(b.kind).cmp(&kind_weight(a.kind));
            if kw != std::cmp::Ordering::Equal {
                return kw;
            }

            // Finally: newest first
            b.created_at.cmp(&a.created_at)
        });
    }

    /// Get suggestion count by priority
    pub fn counts(&self) -> SuggestionCounts {
        let active = self.active_suggestions();
        SuggestionCounts {
            total: active.len(),
            high: active.iter().filter(|s| s.priority == Priority::High).count(),
            medium: active.iter().filter(|s| s.priority == Priority::Medium).count(),
            low: active.iter().filter(|s| s.priority == Priority::Low).count(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SuggestionCounts {
    #[allow(dead_code)]
    pub total: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

/// Convert a detected pattern into a refactoring suggestion, if applicable.
/// 
/// Returns None for patterns that aren't refactoring-related (e.g., TodoMarker).
fn pattern_to_refactoring_suggestion(pattern: &Pattern) -> Option<Suggestion> {
    let (summary, detail, priority) = match pattern.kind {
        PatternKind::LongFunction => {
            let summary = format!(
                "This function is {} - consider breaking it into smaller, focused functions",
                pattern.description
            );
            let detail = "Long functions are harder to test, understand, and maintain. \
                Look for logical sections that could become separate functions with clear names.";
            (summary, detail, Priority::Medium)
        }
        PatternKind::DeepNesting => {
            let summary = "Deeply nested code makes logic hard to follow - consider early returns or extracting helpers".to_string();
            let detail = "Deep nesting often indicates complex conditional logic. \
                Try using early returns (guard clauses) to reduce nesting, \
                or extract nested blocks into well-named helper functions.";
            (summary, detail, Priority::Medium)
        }
        PatternKind::ManyParameters => {
            let summary = format!(
                "{} - consider grouping related parameters into a struct",
                pattern.description
            );
            let detail = "Functions with many parameters are hard to call correctly \
                and suggest the function may be doing too much. \
                Group related parameters into a configuration struct or builder pattern.";
            (summary, detail, Priority::Low)
        }
        PatternKind::GodModule => {
            let summary = format!(
                "{} - consider splitting into focused modules",
                pattern.description
            );
            let detail = "Large files are hard to navigate and often contain \
                multiple responsibilities. Look for natural groupings of \
                functions and types that could become separate modules.";
            (summary, detail, Priority::High)
        }
        PatternKind::DuplicatePattern => {
            let summary = "Duplicate code pattern detected - consider extracting into a shared utility".to_string();
            let detail = "Repeated code makes maintenance harder and increases bug risk. \
                Extract the common pattern into a reusable function or module.";
            (summary, detail, Priority::Medium)
        }
        // These patterns aren't refactoring-related
        PatternKind::MissingErrorHandling
        | PatternKind::UnusedImport
        | PatternKind::TodoMarker => return None,
    };

    Some(
        Suggestion::new(
            SuggestionKind::Refactoring,
            priority,
            pattern.file.clone(),
            summary,
            SuggestionSource::Static,
        )
        .with_line(pattern.line)
        .with_detail(detail.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
    }

    #[test]
    fn test_suggestion_creation() {
        let suggestion = Suggestion::new(
            SuggestionKind::Improvement,
            Priority::High,
            PathBuf::from("test.rs"),
            "Test suggestion".to_string(),
            SuggestionSource::Static,
        );
        
        assert!(!suggestion.dismissed);
        assert!(!suggestion.applied);
    }
}
