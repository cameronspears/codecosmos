//! Codebase indexing engine for Cosmos
//!
//! Uses tree-sitter for multi-language AST parsing to build
//! semantic understanding of the codebase.

pub mod parser;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Go,
    Unknown,
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "py" | "pyi" => Language::Python,
            "go" => Language::Go,
            _ => Language::Unknown,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Language::Rust => "rs",
            Language::JavaScript => "js",
            Language::TypeScript => "ts",
            Language::Python => "py",
            Language::Go => "go",
            Language::Unknown => "??",
        }
    }
}

/// A symbol extracted from the AST (function, struct, class, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub line: usize,
    pub end_line: usize,
    pub complexity: f64,
    pub visibility: Visibility,
}

impl Symbol {
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.line) + 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Enum,
    Interface,
    Trait,
    Module,
    Constant,
    Variable,
}

impl SymbolKind {
    pub fn icon(&self) -> char {
        match self {
            SymbolKind::Function | SymbolKind::Method => 'f',
            SymbolKind::Struct | SymbolKind::Class => 'S',
            SymbolKind::Enum => 'E',
            SymbolKind::Interface | SymbolKind::Trait => 'T',
            SymbolKind::Module => 'M',
            SymbolKind::Constant => 'C',
            SymbolKind::Variable => 'v',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Internal,
}

/// A dependency/import found in the code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub from_file: PathBuf,
    pub import_path: String,
    pub line: usize,
    pub is_external: bool,
}

/// Recognized code patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub kind: PatternKind,
    pub file: PathBuf,
    pub line: usize,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternKind {
    /// Long function (>50 lines)
    LongFunction,
    /// Deeply nested code (>4 levels)
    DeepNesting,
    /// Many parameters (>5)
    ManyParameters,
    /// God class/module (>500 lines)
    GodModule,
    /// Duplicate code pattern
    DuplicatePattern,
    /// Missing error handling
    MissingErrorHandling,
    /// Unused import
    UnusedImport,
    /// TODO/FIXME marker
    TodoMarker,
}

impl PatternKind {
    pub fn severity(&self) -> PatternSeverity {
        match self {
            PatternKind::LongFunction => PatternSeverity::Medium,
            PatternKind::DeepNesting => PatternSeverity::High,
            PatternKind::ManyParameters => PatternSeverity::Low,
            PatternKind::GodModule => PatternSeverity::High,
            PatternKind::DuplicatePattern => PatternSeverity::Medium,
            PatternKind::MissingErrorHandling => PatternSeverity::High,
            PatternKind::UnusedImport => PatternSeverity::Low,
            PatternKind::TodoMarker => PatternSeverity::Info,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            PatternKind::LongFunction => "Function exceeds 50 lines",
            PatternKind::DeepNesting => "Code nesting exceeds 4 levels",
            PatternKind::ManyParameters => "Function has more than 5 parameters",
            PatternKind::GodModule => "File exceeds 500 lines",
            PatternKind::DuplicatePattern => "Similar code pattern detected",
            PatternKind::MissingErrorHandling => "Error handling may be missing",
            PatternKind::UnusedImport => "Import appears unused",
            PatternKind::TodoMarker => "TODO/FIXME marker found",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PatternSeverity {
    Info,
    Low,
    Medium,
    High,
}

/// Index of a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndex {
    pub path: PathBuf,
    pub language: Language,
    pub loc: usize,
    pub sloc: usize, // Source lines (excluding blanks/comments)
    pub symbols: Vec<Symbol>,
    pub dependencies: Vec<Dependency>,
    pub patterns: Vec<Pattern>,
    pub complexity: f64,
    pub last_modified: DateTime<Utc>,
}

impl FileIndex {
    pub fn suggestion_density(&self) -> f64 {
        let pattern_weight: f64 = self.patterns.iter()
            .map(|p| match p.kind.severity() {
                PatternSeverity::High => 3.0,
                PatternSeverity::Medium => 2.0,
                PatternSeverity::Low => 1.0,
                PatternSeverity::Info => 0.5,
            })
            .sum();
        
        // Normalize by file size
        if self.loc > 0 {
            pattern_weight / (self.loc as f64 / 100.0)
        } else {
            0.0
        }
    }

    pub fn priority_indicator(&self) -> char {
        let density = self.suggestion_density();
        if density > 5.0 {
            '\u{25CF}' // ● High
        } else if density > 2.0 {
            '\u{25D0}' // ◐ Medium
        } else if density > 0.0 {
            '\u{25CB}' // ○ Low
        } else {
            ' ' // None
        }
    }
}

/// The complete codebase index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseIndex {
    pub root: PathBuf,
    pub files: HashMap<PathBuf, FileIndex>,
    pub symbols: Vec<Symbol>,
    pub dependencies: Vec<Dependency>,
    pub patterns: Vec<Pattern>,
    pub cached_at: DateTime<Utc>,
}

impl CodebaseIndex {
    /// Create a new index for a codebase
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        let mut index = Self {
            root: root.to_path_buf(),
            files: HashMap::new(),
            symbols: Vec::new(),
            dependencies: Vec::new(),
            patterns: Vec::new(),
            cached_at: Utc::now(),
        };

        index.scan(root)?;
        Ok(index)
    }

    /// Scan directory and index all supported files
    fn scan(&mut self, root: &Path) -> anyhow::Result<()> {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_ignored(e.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            
            let language = Language::from_extension(ext);
            if language == Language::Unknown {
                continue;
            }

            if let Ok(file_index) = self.index_file(path, language) {
                // Aggregate symbols, dependencies, patterns
                self.symbols.extend(file_index.symbols.clone());
                self.dependencies.extend(file_index.dependencies.clone());
                self.patterns.extend(file_index.patterns.clone());
                
                let rel_path = path.strip_prefix(root).unwrap_or(path).to_path_buf();
                self.files.insert(rel_path, file_index);
            }
        }

        Ok(())
    }

    /// Index a single file
    fn index_file(&self, path: &Path, language: Language) -> anyhow::Result<FileIndex> {
        let content = std::fs::read_to_string(path)?;
        let metadata = std::fs::metadata(path)?;
        let modified = metadata.modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        let loc = content.lines().count();
        let sloc = content.lines()
            .filter(|l| !l.trim().is_empty())
            .count();

        // Parse with tree-sitter
        let (symbols, deps) = parser::parse_file(path, &content, language)?;
        
        // Detect patterns
        let mut patterns = Vec::new();
        
        // Check for long functions
        for sym in &symbols {
            if matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
                if sym.line_count() > 50 {
                    patterns.push(Pattern {
                        kind: PatternKind::LongFunction,
                        file: path.to_path_buf(),
                        line: sym.line,
                        description: format!("{} is {} lines", sym.name, sym.line_count()),
                    });
                }
            }
        }
        
        // Check for god module
        if loc > 500 {
            patterns.push(Pattern {
                kind: PatternKind::GodModule,
                file: path.to_path_buf(),
                line: 1,
                description: format!("File has {} lines", loc),
            });
        }

        // Scan for TODO/FIXME
        for (i, line) in content.lines().enumerate() {
            let upper = line.to_uppercase();
            if upper.contains("TODO") || upper.contains("FIXME") || upper.contains("HACK") {
                patterns.push(Pattern {
                    kind: PatternKind::TodoMarker,
                    file: path.to_path_buf(),
                    line: i + 1,
                    description: line.trim().to_string(),
                });
            }
        }

        // Calculate complexity (simplified cyclomatic)
        let complexity = calculate_complexity(&content, language);

        Ok(FileIndex {
            path: path.to_path_buf(),
            language,
            loc,
            sloc,
            symbols,
            dependencies: deps,
            patterns,
            complexity,
            last_modified: modified,
        })
    }

    /// Get files sorted by suggestion density (most actionable first)
    pub fn files_by_priority(&self) -> Vec<(&PathBuf, &FileIndex)> {
        let mut files: Vec<_> = self.files.iter().collect();
        files.sort_by(|a, b| {
            b.1.suggestion_density()
                .partial_cmp(&a.1.suggestion_density())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        files
    }

    /// Get total statistics
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            file_count: self.files.len(),
            total_loc: self.files.values().map(|f| f.loc).sum(),
            total_sloc: self.files.values().map(|f| f.sloc).sum(),
            symbol_count: self.symbols.len(),
            pattern_count: self.patterns.len(),
            high_priority_patterns: self.patterns.iter()
                .filter(|p| p.kind.severity() >= PatternSeverity::High)
                .count(),
        }
    }

    /// Get file tree structure
    pub fn file_tree(&self) -> FileTree {
        let mut tree = FileTree::new();
        for path in self.files.keys() {
            tree.insert(path, &self.files[path]);
        }
        tree
    }
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub file_count: usize,
    pub total_loc: usize,
    pub total_sloc: usize,
    pub symbol_count: usize,
    pub pattern_count: usize,
    pub high_priority_patterns: usize,
}

/// File tree structure for UI display
#[derive(Debug, Clone, Default)]
pub struct FileTree {
    pub entries: Vec<FileTreeEntry>,
}

#[derive(Debug, Clone)]
pub struct FileTreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
    pub priority: char,
    pub expanded: bool,
    pub children: Vec<FileTreeEntry>,
}

impl FileTree {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn insert(&mut self, path: &Path, file_index: &FileIndex) {
        let components: Vec<_> = path.components().collect();
        self.insert_recursive(&mut self.entries.clone(), &components, 0, file_index);
    }

    fn insert_recursive(
        &mut self,
        entries: &mut Vec<FileTreeEntry>,
        components: &[std::path::Component],
        depth: usize,
        file_index: &FileIndex,
    ) {
        // Simplified tree building - actual implementation would be more complex
        if components.is_empty() {
            return;
        }

        let name = components[0].as_os_str().to_string_lossy().to_string();
        let is_last = components.len() == 1;
        
        let entry = FileTreeEntry {
            name,
            path: file_index.path.clone(),
            is_dir: !is_last,
            depth,
            priority: if is_last { file_index.priority_indicator() } else { ' ' },
            expanded: true,
            children: Vec::new(),
        };

        self.entries.push(entry);
    }

    /// Flatten tree for display
    pub fn flatten(&self) -> Vec<FlatTreeEntry> {
        let mut result = Vec::new();
        self.flatten_recursive(&self.entries, &mut result);
        result
    }

    fn flatten_recursive(&self, entries: &[FileTreeEntry], result: &mut Vec<FlatTreeEntry>) {
        for entry in entries {
            result.push(FlatTreeEntry {
                name: entry.name.clone(),
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                depth: entry.depth,
                priority: entry.priority,
            });
            if entry.expanded {
                self.flatten_recursive(&entry.children, result);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlatTreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
    pub priority: char,
}

/// Calculate cyclomatic complexity (simplified)
fn calculate_complexity(content: &str, _language: Language) -> f64 {
    // Count decision points
    let decision_keywords = [
        "if ", "else ", "elif ", "for ", "while ", "match ", 
        "case ", "catch ", "&&", "||", "?", "try ", "switch "
    ];
    
    let mut complexity = 1.0; // Base complexity
    
    for keyword in &decision_keywords {
        complexity += content.matches(keyword).count() as f64;
    }
    
    complexity
}

/// Check if a path should be ignored
fn is_ignored(path: &Path) -> bool {
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    
    // Common ignore patterns
    let ignored = [
        "target", "node_modules", ".git", ".svn", ".hg",
        "dist", "build", "__pycache__", ".pytest_cache",
        "vendor", ".idea", ".vscode", ".cosmos",
    ];
    
    ignored.contains(&name) || name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("js"), Language::JavaScript);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("txt"), Language::Unknown);
    }

    #[test]
    fn test_complexity_calculation() {
        let code = "if x { } else { } for i in items { if y { } }";
        let complexity = calculate_complexity(code, Language::Rust);
        assert!(complexity > 1.0);
    }

    #[test]
    fn test_pattern_severity() {
        assert!(PatternKind::DeepNesting.severity() > PatternKind::UnusedImport.severity());
    }
}
