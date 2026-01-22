use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }

    let char_count = s.chars().count();
    if char_count <= max {
        return s.to_string();
    }

    if max <= 3 {
        return s.chars().take(max).collect();
    }

    let truncated: String = s.chars().take(max - 3).collect();
    format!("{}...", truncated)
}

#[derive(Debug)]
pub struct CommandRunResult {
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

pub fn run_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<CommandRunResult, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start command: {}", e))?;
    child.kill_on_drop(true);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture stderr".to_string())?;

    let stdout_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let mut reader = BufReader::new(stdout);
        let _ = reader.read_to_end(&mut buf);
        buf
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    let _ = child.kill();
                    match child.wait() {
                        Ok(status) => break Some(status),
                        Err(_) => break None,
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("Failed to wait for command: {}", e)),
        }
    };

    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();

    Ok(CommandRunResult {
        status,
        stdout: String::from_utf8_lossy(&stdout_bytes).to_string(),
        stderr: String::from_utf8_lossy(&stderr_bytes).to_string(),
        timed_out,
    })
}

pub struct RepoPath {
    pub absolute: PathBuf,
    pub relative: PathBuf,
}

pub fn resolve_repo_path(repo_root: &Path, candidate: &Path) -> Result<RepoPath, String> {
    if candidate.as_os_str().is_empty() {
        return Err("Path is empty".to_string());
    }
    if candidate.is_absolute() {
        return Err(format!(
            "Absolute paths are not allowed: {}",
            candidate.display()
        ));
    }
    if candidate
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(format!(
            "Parent traversal is not allowed: {}",
            candidate.display()
        ));
    }

    let root = repo_root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo root: {}", e))?;
    let joined = root.join(candidate);

    if !joined.exists() {
        return Err(format!("File does not exist: {}", candidate.display()));
    }

    let absolute = joined
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path {}: {}", candidate.display(), e))?;

    if !absolute.starts_with(&root) {
        return Err(format!("Path escapes repository: {}", candidate.display()));
    }

    let relative = absolute
        .strip_prefix(&root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| candidate.to_path_buf());

    Ok(RepoPath { absolute, relative })
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn test_truncate_unicode_safe() {
        let input = "ééééé";
        assert_eq!(truncate(input, 4), "é...");
    }

    #[test]
    fn test_truncate_small_max() {
        let input = "こんにちは";
        assert_eq!(truncate(input, 3), "こんに");
        assert_eq!(truncate(input, 0), "");
    }
}
