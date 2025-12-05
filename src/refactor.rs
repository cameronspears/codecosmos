//! Multi-file refactoring support
//!
//! Handles splitting large files into smaller modules and other multi-file operations.

use crate::diff::UnifiedDiff;
use std::fs;
use std::path::Path;

/// A complete refactoring plan with multiple file operations
#[derive(Debug, Clone, PartialEq)]
pub struct RefactorPlan {
    pub description: String,
    pub operations: Vec<FileOperation>,
}

impl RefactorPlan {
    pub fn new(description: &str) -> Self {
        Self {
            description: description.to_string(),
            operations: Vec::new(),
        }
    }

    /// Get summary stats for the plan
    pub fn stats(&self) -> RefactorStats {
        let mut stats = RefactorStats::default();
        for op in &self.operations {
            match op {
                FileOperation::Create { .. } => stats.creates += 1,
                FileOperation::Modify { .. } => stats.modifies += 1,
                FileOperation::Delete { .. } => stats.deletes += 1,
                FileOperation::Rename { .. } => stats.renames += 1,
            }
        }
        stats
    }
}

#[derive(Debug, Clone, Default)]
pub struct RefactorStats {
    pub creates: usize,
    pub modifies: usize,
    pub deletes: usize,
    pub renames: usize,
}

/// A single file operation in a refactoring plan
#[derive(Debug, Clone, PartialEq)]
pub enum FileOperation {
    /// Create a new file with the given content
    Create { path: String, content: String },
    /// Modify an existing file with a unified diff
    Modify { path: String, diff: UnifiedDiff },
    /// Delete a file
    Delete { path: String },
    /// Rename/move a file
    Rename { from: String, to: String },
}

impl FileOperation {
    pub fn path(&self) -> &str {
        match self {
            FileOperation::Create { path, .. } => path,
            FileOperation::Modify { path, .. } => path,
            FileOperation::Delete { path } => path,
            FileOperation::Rename { from, .. } => from,
        }
    }

    pub fn operation_type(&self) -> &'static str {
        match self {
            FileOperation::Create { .. } => "CREATE",
            FileOperation::Modify { .. } => "MODIFY",
            FileOperation::Delete { .. } => "DELETE",
            FileOperation::Rename { .. } => "RENAME",
        }
    }

    pub fn preview_lines(&self) -> Vec<String> {
        match self {
            FileOperation::Create { content, .. } => {
                content.lines().take(20).map(|l| format!("+{}", l)).collect()
            }
            FileOperation::Modify { diff, .. } => {
                let mut lines = Vec::new();
                for hunk in &diff.hunks {
                    lines.push(format!(
                        "@@ -{},{} +{},{} @@",
                        hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
                    ));
                    for line in &hunk.lines {
                        match line {
                            crate::diff::DiffLine::Add(s) => lines.push(format!("+{}", s)),
                            crate::diff::DiffLine::Remove(s) => lines.push(format!("-{}", s)),
                            crate::diff::DiffLine::Context(s) => lines.push(format!(" {}", s)),
                        }
                    }
                }
                lines.into_iter().take(20).collect()
            }
            FileOperation::Delete { path } => {
                vec![format!("Delete file: {}", path)]
            }
            FileOperation::Rename { from, to } => {
                vec![format!("{} -> {}", from, to)]
            }
        }
    }
}

/// Parse AI output in multi-file diff format into a RefactorPlan
///
/// Expected format:
/// ```text
/// === CREATE path/to/new/file.rs ===
/// content of the new file
/// goes here
///
/// === MODIFY path/to/existing/file.rs ===
/// --- a/path/to/existing/file.rs
/// +++ b/path/to/existing/file.rs
/// @@ -1,5 +1,6 @@
///  context
/// -removed
/// +added
///
/// === DELETE path/to/old/file.rs ===
///
/// === RENAME old/path.rs -> new/path.rs ===
/// ```
pub fn parse_multi_file_diff(input: &str) -> Result<RefactorPlan, String> {
    let mut plan = RefactorPlan::new("AI-generated refactoring plan");
    let mut current_op: Option<PendingOp> = None;
    let mut content_lines: Vec<String> = Vec::new();

    for line in input.lines() {
        // Check for operation headers
        if line.starts_with("=== CREATE ") && line.ends_with(" ===") {
            // Finalize previous operation
            if let Some(op) = current_op.take() {
                plan.operations.push(finalize_op(op, &content_lines)?);
                content_lines.clear();
            }
            let path = line
                .trim_start_matches("=== CREATE ")
                .trim_end_matches(" ===")
                .to_string();
            current_op = Some(PendingOp::Create { path });
        } else if line.starts_with("=== MODIFY ") && line.ends_with(" ===") {
            if let Some(op) = current_op.take() {
                plan.operations.push(finalize_op(op, &content_lines)?);
                content_lines.clear();
            }
            let path = line
                .trim_start_matches("=== MODIFY ")
                .trim_end_matches(" ===")
                .to_string();
            current_op = Some(PendingOp::Modify { path });
        } else if line.starts_with("=== DELETE ") && line.ends_with(" ===") {
            if let Some(op) = current_op.take() {
                plan.operations.push(finalize_op(op, &content_lines)?);
                content_lines.clear();
            }
            let path = line
                .trim_start_matches("=== DELETE ")
                .trim_end_matches(" ===")
                .to_string();
            plan.operations.push(FileOperation::Delete { path });
            current_op = None;
        } else if line.starts_with("=== RENAME ") && line.ends_with(" ===") {
            if let Some(op) = current_op.take() {
                plan.operations.push(finalize_op(op, &content_lines)?);
                content_lines.clear();
            }
            let rename_part = line
                .trim_start_matches("=== RENAME ")
                .trim_end_matches(" ===");
            if let Some((from, to)) = rename_part.split_once(" -> ") {
                plan.operations.push(FileOperation::Rename {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
            current_op = None;
        } else if current_op.is_some() {
            content_lines.push(line.to_string());
        }
    }

    // Finalize last operation
    if let Some(op) = current_op {
        plan.operations.push(finalize_op(op, &content_lines)?);
    }

    if plan.operations.is_empty() {
        return Err("No operations found in refactoring plan".to_string());
    }

    Ok(plan)
}

#[derive(Debug)]
enum PendingOp {
    Create { path: String },
    Modify { path: String },
}

fn finalize_op(op: PendingOp, lines: &[String]) -> Result<FileOperation, String> {
    match op {
        PendingOp::Create { path } => {
            let content = lines.join("\n");
            Ok(FileOperation::Create { path, content })
        }
        PendingOp::Modify { path } => {
            let diff_text = lines.join("\n");
            let diff = crate::diff::parse_unified_diff(&diff_text)?;
            Ok(FileOperation::Modify { path, diff })
        }
    }
}

/// Backup info for rollback
#[derive(Debug)]
struct BackupEntry {
    path: String,
    original_content: Option<String>, // None if file didn't exist
}

/// Apply a refactoring plan atomically with rollback support
pub fn apply_refactor_plan(repo_path: &Path, plan: &RefactorPlan) -> Result<(), String> {
    let mut backups: Vec<BackupEntry> = Vec::new();

    // First, create backups of all files that will be modified or deleted
    for op in &plan.operations {
        match op {
            FileOperation::Create { path, .. } => {
                let full_path = repo_path.join(path);
                let original = if full_path.exists() {
                    Some(fs::read_to_string(&full_path).map_err(|e| e.to_string())?)
                } else {
                    None
                };
                backups.push(BackupEntry {
                    path: path.clone(),
                    original_content: original,
                });
            }
            FileOperation::Modify { path, .. } => {
                let full_path = repo_path.join(path);
                let original = fs::read_to_string(&full_path).map_err(|e| {
                    format!("Failed to read {} for backup: {}", path, e)
                })?;
                backups.push(BackupEntry {
                    path: path.clone(),
                    original_content: Some(original),
                });
            }
            FileOperation::Delete { path } => {
                let full_path = repo_path.join(path);
                if full_path.exists() {
                    let original = fs::read_to_string(&full_path).map_err(|e| {
                        format!("Failed to read {} for backup: {}", path, e)
                    })?;
                    backups.push(BackupEntry {
                        path: path.clone(),
                        original_content: Some(original),
                    });
                }
            }
            FileOperation::Rename { from, .. } => {
                let full_path = repo_path.join(from);
                if full_path.exists() {
                    let original = fs::read_to_string(&full_path).map_err(|e| {
                        format!("Failed to read {} for backup: {}", from, e)
                    })?;
                    backups.push(BackupEntry {
                        path: from.clone(),
                        original_content: Some(original),
                    });
                }
            }
        }
    }

    // Apply all operations
    let result = apply_operations(repo_path, plan);

    // If any operation failed, rollback
    if let Err(e) = result {
        rollback(repo_path, &backups);
        return Err(format!("Refactoring failed, rolled back: {}", e));
    }

    Ok(())
}

fn apply_operations(repo_path: &Path, plan: &RefactorPlan) -> Result<(), String> {
    for op in &plan.operations {
        match op {
            FileOperation::Create { path, content } => {
                let full_path = repo_path.join(path);
                // Create parent directories if needed
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create directory for {}: {}", path, e)
                    })?;
                }
                fs::write(&full_path, content).map_err(|e| {
                    format!("Failed to create {}: {}", path, e)
                })?;
            }
            FileOperation::Modify { path, diff } => {
                let full_path = repo_path.join(path);
                crate::diff::apply_diff_to_file(&full_path, diff)?;
            }
            FileOperation::Delete { path } => {
                let full_path = repo_path.join(path);
                if full_path.exists() {
                    fs::remove_file(&full_path).map_err(|e| {
                        format!("Failed to delete {}: {}", path, e)
                    })?;
                }
            }
            FileOperation::Rename { from, to } => {
                let from_path = repo_path.join(from);
                let to_path = repo_path.join(to);
                // Create parent directories if needed
                if let Some(parent) = to_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create directory for {}: {}", to, e)
                    })?;
                }
                fs::rename(&from_path, &to_path).map_err(|e| {
                    format!("Failed to rename {} to {}: {}", from, to, e)
                })?;
            }
        }
    }
    Ok(())
}

fn rollback(repo_path: &Path, backups: &[BackupEntry]) {
    for backup in backups.iter().rev() {
        let full_path = repo_path.join(&backup.path);
        match &backup.original_content {
            Some(content) => {
                // Restore original content
                let _ = fs::write(&full_path, content);
            }
            None => {
                // File didn't exist before, remove it
                let _ = fs::remove_file(&full_path);
            }
        }
    }
}

/// Generate a preview of the refactoring plan
pub fn preview_refactor(plan: &RefactorPlan) -> String {
    let mut preview = String::new();
    let stats = plan.stats();

    preview.push_str(&format!("# {}\n\n", plan.description));
    preview.push_str(&format!(
        "Summary: {} creates, {} modifies, {} deletes, {} renames\n\n",
        stats.creates, stats.modifies, stats.deletes, stats.renames
    ));

    for (i, op) in plan.operations.iter().enumerate() {
        preview.push_str(&format!(
            "## {}. {} {}\n",
            i + 1,
            op.operation_type(),
            op.path()
        ));
        for line in op.preview_lines() {
            preview.push_str(&format!("{}\n", line));
        }
        preview.push('\n');
    }

    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_operation() {
        let input = r#"=== CREATE src/new_file.rs ===
pub fn hello() {
    println!("Hello!");
}
"#;
        let plan = parse_multi_file_diff(input).unwrap();
        assert_eq!(plan.operations.len(), 1);
        match &plan.operations[0] {
            FileOperation::Create { path, content } => {
                assert_eq!(path, "src/new_file.rs");
                assert!(content.contains("pub fn hello()"));
            }
            _ => panic!("Expected Create operation"),
        }
    }

    #[test]
    fn test_parse_multiple_operations() {
        let input = r#"=== CREATE src/a.rs ===
content a

=== CREATE src/b.rs ===
content b

=== DELETE src/old.rs ===
"#;
        let plan = parse_multi_file_diff(input).unwrap();
        assert_eq!(plan.operations.len(), 3);
        
        let stats = plan.stats();
        assert_eq!(stats.creates, 2);
        assert_eq!(stats.deletes, 1);
    }

    #[test]
    fn test_parse_rename_operation() {
        let input = "=== RENAME old/path.rs -> new/path.rs ===\n";
        let plan = parse_multi_file_diff(input).unwrap();
        assert_eq!(plan.operations.len(), 1);
        match &plan.operations[0] {
            FileOperation::Rename { from, to } => {
                assert_eq!(from, "old/path.rs");
                assert_eq!(to, "new/path.rs");
            }
            _ => panic!("Expected Rename operation"),
        }
    }
}

