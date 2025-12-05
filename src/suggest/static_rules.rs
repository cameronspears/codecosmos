//! Static rule-based suggestions (no LLM cost)
//!
//! These rules analyze code patterns and generate suggestions
//! without any API calls, making them completely free.

use super::{Priority, Suggestion, SuggestionKind, SuggestionSource};
use crate::index::{FileIndex, Language, PatternKind, PatternSeverity, Symbol, SymbolKind};
use std::path::PathBuf;

/// Analyze a file and generate static suggestions
pub fn analyze_file(path: &PathBuf, file_index: &FileIndex) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    // File-level checks
    suggestions.extend(check_file_size(path, file_index));
    suggestions.extend(check_complexity(path, file_index));

    // Symbol-level checks
    for symbol in &file_index.symbols {
        suggestions.extend(check_function_length(path, symbol));
        suggestions.extend(check_function_complexity(path, symbol));
    }

    // Pattern-based suggestions
    for pattern in &file_index.patterns {
        if let Some(suggestion) = pattern_to_suggestion(path, pattern) {
            suggestions.push(suggestion);
        }
    }

    // Language-specific checks
    suggestions.extend(language_specific_checks(path, file_index));

    suggestions
}

/// Check for overly large files
fn check_file_size(path: &PathBuf, file_index: &FileIndex) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    if file_index.loc > 1000 {
        suggestions.push(
            Suggestion::new(
                SuggestionKind::Improvement,
                Priority::High,
                path.clone(),
                format!("Large file ({} lines) - consider splitting into modules", file_index.loc),
                SuggestionSource::Static,
            )
            .with_detail(format!(
                "Files over 1000 lines become difficult to maintain. \
                 This file has {} lines of code ({} non-blank). \
                 Consider extracting related functionality into separate modules.",
                file_index.loc, file_index.sloc
            )),
        );
    } else if file_index.loc > 500 {
        suggestions.push(
            Suggestion::new(
                SuggestionKind::Quality,
                Priority::Medium,
                path.clone(),
                format!("Growing file ({} lines) - monitor complexity", file_index.loc),
                SuggestionSource::Static,
            )
            .with_detail(
                "This file is approaching the 500+ line threshold where \
                 maintainability starts to decline. Consider if any \
                 functionality could be extracted."
                    .to_string(),
            ),
        );
    }

    suggestions
}

/// Check overall file complexity
fn check_complexity(path: &PathBuf, file_index: &FileIndex) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    // High average complexity
    let function_count = file_index
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .count();

    if function_count > 0 {
        let avg_complexity = file_index.complexity / function_count as f64;

        if avg_complexity > 15.0 {
            suggestions.push(
                Suggestion::new(
                    SuggestionKind::Improvement,
                    Priority::High,
                    path.clone(),
                    format!(
                        "High average complexity ({:.1}) - functions may be too complex",
                        avg_complexity
                    ),
                    SuggestionSource::Static,
                )
                .with_detail(
                    "High cyclomatic complexity makes code harder to test and maintain. \
                     Consider breaking complex functions into smaller, focused units."
                        .to_string(),
                ),
            );
        }
    }

    // Too many functions in one file
    if function_count > 30 {
        suggestions.push(
            Suggestion::new(
                SuggestionKind::Quality,
                Priority::Medium,
                path.clone(),
                format!(
                    "Many functions ({}) - consider organizing into modules",
                    function_count
                ),
                SuggestionSource::Static,
            )
            .with_detail(
                "Having many functions in a single file can indicate mixed responsibilities. \
                 Group related functions into separate modules for better organization."
                    .to_string(),
            ),
        );
    }

    suggestions
}

/// Check individual function length
fn check_function_length(path: &PathBuf, symbol: &Symbol) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
        return suggestions;
    }

    let lines = symbol.line_count();

    if lines > 100 {
        suggestions.push(
            Suggestion::new(
                SuggestionKind::Improvement,
                Priority::High,
                path.clone(),
                format!("`{}` is {} lines - strongly consider refactoring", symbol.name, lines),
                SuggestionSource::Static,
            )
            .with_line(symbol.line)
            .with_detail(format!(
                "The function `{}` spans {} lines, which is very long. \
                 Long functions are harder to understand, test, and maintain. \
                 Consider extracting logical sections into helper functions.",
                symbol.name, lines
            )),
        );
    } else if lines > 50 {
        suggestions.push(
            Suggestion::new(
                SuggestionKind::Quality,
                Priority::Medium,
                path.clone(),
                format!("`{}` is {} lines - consider splitting", symbol.name, lines),
                SuggestionSource::Static,
            )
            .with_line(symbol.line)
            .with_detail(format!(
                "The function `{}` is getting long at {} lines. \
                 Functions over 50 lines often benefit from being broken down.",
                symbol.name, lines
            )),
        );
    }

    suggestions
}

/// Check function complexity
fn check_function_complexity(path: &PathBuf, symbol: &Symbol) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
        return suggestions;
    }

    if symbol.complexity > 20.0 {
        suggestions.push(
            Suggestion::new(
                SuggestionKind::Improvement,
                Priority::High,
                path.clone(),
                format!(
                    "`{}` has high complexity ({:.0}) - simplify logic",
                    symbol.name, symbol.complexity
                ),
                SuggestionSource::Static,
            )
            .with_line(symbol.line)
            .with_detail(format!(
                "The function `{}` has a cyclomatic complexity of {:.0}. \
                 High complexity often indicates too many code paths. \
                 Consider using early returns, extracting conditions, or \
                 breaking into smaller functions.",
                symbol.name, symbol.complexity
            )),
        );
    } else if symbol.complexity > 10.0 {
        suggestions.push(
            Suggestion::new(
                SuggestionKind::Quality,
                Priority::Low,
                path.clone(),
                format!(
                    "`{}` has moderate complexity ({:.0})",
                    symbol.name, symbol.complexity
                ),
                SuggestionSource::Static,
            )
            .with_line(symbol.line),
        );
    }

    suggestions
}

/// Convert a detected pattern to a suggestion
fn pattern_to_suggestion(path: &PathBuf, pattern: &crate::index::Pattern) -> Option<Suggestion> {
    let (kind, priority, summary, detail) = match pattern.kind {
        PatternKind::LongFunction => (
            SuggestionKind::Improvement,
            Priority::from_severity(pattern.kind.severity()),
            pattern.description.clone(),
            Some(
                "Long functions are harder to understand and test. \
                 Break them into smaller, focused functions."
                    .to_string(),
            ),
        ),
        PatternKind::DeepNesting => (
            SuggestionKind::Improvement,
            Priority::High,
            "Deep nesting detected - flatten with early returns".to_string(),
            Some(
                "Deeply nested code is hard to follow. Use early returns, \
                 guard clauses, or extract nested logic into helper functions."
                    .to_string(),
            ),
        ),
        PatternKind::ManyParameters => (
            SuggestionKind::Quality,
            Priority::Medium,
            "Function has many parameters - consider using a struct".to_string(),
            Some(
                "Functions with many parameters are hard to call correctly. \
                 Consider grouping related parameters into a struct or builder."
                    .to_string(),
            ),
        ),
        PatternKind::GodModule => (
            SuggestionKind::Improvement,
            Priority::High,
            format!("Large module - {}", pattern.description),
            Some(
                "This module has grown large and likely has multiple responsibilities. \
                 Consider splitting into focused sub-modules."
                    .to_string(),
            ),
        ),
        PatternKind::DuplicatePattern => (
            SuggestionKind::Improvement,
            Priority::Medium,
            "Potential code duplication detected".to_string(),
            Some(
                "Similar code patterns found. Consider extracting to a shared \
                 function or using abstractions to reduce duplication."
                    .to_string(),
            ),
        ),
        PatternKind::MissingErrorHandling => (
            SuggestionKind::BugFix,
            Priority::High,
            "Error handling may be missing".to_string(),
            Some(
                "This code path may not properly handle errors. \
                 Add appropriate error handling to prevent runtime failures."
                    .to_string(),
            ),
        ),
        PatternKind::UnusedImport => (
            SuggestionKind::Quality,
            Priority::Low,
            "Unused import detected".to_string(),
            Some("Remove unused imports to keep the code clean.".to_string()),
        ),
        PatternKind::TodoMarker => {
            let todo_text = &pattern.description;
            let priority = if todo_text.to_uppercase().contains("FIXME")
                || todo_text.to_uppercase().contains("BUG")
            {
                Priority::High
            } else if todo_text.to_uppercase().contains("HACK") {
                Priority::Medium
            } else {
                Priority::Low
            };

            (
                SuggestionKind::Quality,
                priority,
                format!("TODO marker: {}", truncate(todo_text, 50)),
                Some(format!("Found: {}", todo_text)),
            )
        }
    };

    let mut suggestion = Suggestion::new(kind, priority, path.clone(), summary, SuggestionSource::Static)
        .with_line(pattern.line);

    if let Some(d) = detail {
        suggestion = suggestion.with_detail(d);
    }

    Some(suggestion)
}

/// Language-specific static checks
fn language_specific_checks(path: &PathBuf, file_index: &FileIndex) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    match file_index.language {
        Language::Rust => {
            // Check for unwrap() usage
            // This would require content analysis
        }
        Language::JavaScript | Language::TypeScript => {
            // Check for console.log
            // Check for var usage
        }
        Language::Python => {
            // Check for except: (bare except)
        }
        Language::Go => {
            // Check for ignored error returns
        }
        Language::Unknown => {}
    }

    // Check for missing tests based on file naming
    let path_str = path.to_string_lossy();
    if !path_str.contains("test")
        && !path_str.contains("spec")
        && !path_str.contains("_test")
        && file_index.symbols.len() > 3
    {
        let public_functions = file_index
            .symbols
            .iter()
            .filter(|s| {
                matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                    && matches!(s.visibility, crate::index::Visibility::Public)
            })
            .count();

        if public_functions >= 3 {
            suggestions.push(
                Suggestion::new(
                    SuggestionKind::Testing,
                    Priority::Medium,
                    path.clone(),
                    format!(
                        "{} public functions without apparent tests",
                        public_functions
                    ),
                    SuggestionSource::Static,
                )
                .with_detail(
                    "This file has several public functions but no visible test file. \
                     Consider adding tests to ensure correctness."
                        .to_string(),
                ),
            );
        }
    }

    suggestions
}

/// Truncate a string for display
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Pattern, Symbol, SymbolKind, Visibility};

    #[test]
    fn test_function_length_check() {
        let symbol = Symbol {
            name: "long_function".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("test.rs"),
            line: 1,
            end_line: 120,
            complexity: 5.0,
            visibility: Visibility::Public,
        };

        let path = PathBuf::from("test.rs");
        let suggestions = check_function_length(&path, &symbol);

        assert!(!suggestions.is_empty());
        assert!(suggestions[0].priority == Priority::High);
    }

    #[test]
    fn test_todo_pattern_suggestion() {
        let pattern = crate::index::Pattern {
            kind: PatternKind::TodoMarker,
            file: PathBuf::from("test.rs"),
            line: 10,
            description: "TODO: implement this feature".to_string(),
        };

        let path = PathBuf::from("test.rs");
        let suggestion = pattern_to_suggestion(&path, &pattern);

        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().priority == Priority::Low);
    }

    #[test]
    fn test_fixme_is_high_priority() {
        let pattern = crate::index::Pattern {
            kind: PatternKind::TodoMarker,
            file: PathBuf::from("test.rs"),
            line: 10,
            description: "FIXME: this is broken".to_string(),
        };

        let path = PathBuf::from("test.rs");
        let suggestion = pattern_to_suggestion(&path, &pattern);

        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().priority == Priority::High);
    }
}
