use git2::Repository;
use std::path::Path;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_SHA: &str = env!("COSMOS_GIT_SHA");
pub const BUILD_TIME: &str = env!("COSMOS_BUILD_TIME");
pub const BUILD_PROFILE: &str = env!("COSMOS_BUILD_PROFILE");
pub const MANIFEST_DIR: &str = env!("COSMOS_MANIFEST_DIR");

pub struct StaleBuildInfo {
    pub build: String,
    pub current: String,
}

pub fn print_build_info() {
    println!("cosmos {}", VERSION);
    println!("git: {}", GIT_SHA);
    println!("built: {}", BUILD_TIME);
    println!("profile: {}", BUILD_PROFILE);
}

pub fn stale_build_notice() -> Option<StaleBuildInfo> {
    let manifest_dir = Path::new(MANIFEST_DIR);
    if !manifest_dir.exists() {
        return None;
    }

    let build_base = GIT_SHA.split('+').next().unwrap_or("");
    if build_base.is_empty() || build_base == "unknown" {
        return None;
    }

    let current = current_git_id(manifest_dir, build_base.len())?;
    let current_base = current.split('+').next().unwrap_or("");

    if current_base == build_base {
        None
    } else {
        Some(StaleBuildInfo {
            build: GIT_SHA.to_string(),
            current,
        })
    }
}

fn current_git_id(manifest_dir: &Path, short_len: usize) -> Option<String> {
    let repo = Repository::open(manifest_dir).ok()?;
    let head = repo.head().ok()?;
    let oid = head.target()?;
    let mut id = short_oid(oid, short_len);
    if is_dirty(&repo).unwrap_or(false) {
        id.push_str("+dirty");
    }
    Some(id)
}

fn short_oid(oid: git2::Oid, short_len: usize) -> String {
    let full = oid.to_string();
    let len = short_len.min(full.len()).max(1);
    full.chars().take(len).collect()
}

fn is_dirty(repo: &Repository) -> Option<bool> {
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut options)).ok()?;
    Some(!statuses.is_empty())
}
