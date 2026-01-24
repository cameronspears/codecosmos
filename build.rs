use chrono::{SecondsFormat, TimeZone, Utc};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));

    let git_sha = git_output(&manifest_dir, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|| "unknown".to_string());
    let git_dirty = git_is_dirty(&manifest_dir).unwrap_or(false);
    let git_id = if git_dirty {
        format!("{}+dirty", git_sha)
    } else {
        git_sha
    };

    let build_time = build_time_string();
    let build_profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=COSMOS_GIT_SHA={}", git_id);
    println!("cargo:rustc-env=COSMOS_BUILD_TIME={}", build_time);
    println!("cargo:rustc-env=COSMOS_BUILD_PROFILE={}", build_profile);
    println!(
        "cargo:rustc-env=COSMOS_MANIFEST_DIR={}",
        manifest_dir.display()
    );

    emit_git_rerun(&manifest_dir);
}

fn git_output(manifest_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(manifest_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn git_is_dirty(manifest_dir: &Path) -> Option<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(manifest_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(!output.stdout.is_empty())
}

fn build_time_string() -> String {
    if let Ok(value) = env::var("SOURCE_DATE_EPOCH") {
        if let Ok(epoch) = value.parse::<i64>() {
            return format_epoch(epoch);
        }
    }
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn format_epoch(secs: i64) -> String {
    match Utc.timestamp_opt(secs, 0).single() {
        Some(dt) => dt.to_rfc3339_opts(SecondsFormat::Secs, true),
        None => "unknown".to_string(),
    }
}

fn emit_git_rerun(manifest_dir: &Path) {
    let git_dir = manifest_dir.join(".git");
    let head_path = git_dir.join("HEAD");
    if head_path.exists() {
        println!("cargo:rerun-if-changed={}", head_path.display());
    }

    let head = match std::fs::read_to_string(&head_path) {
        Ok(content) => content,
        Err(_) => return,
    };

    if let Some(ref_path) = head.strip_prefix("ref: ") {
        let ref_path = git_dir.join(ref_path.trim());
        if ref_path.exists() {
            println!("cargo:rerun-if-changed={}", ref_path.display());
        }
    }
}
