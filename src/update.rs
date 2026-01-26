//! Self-update functionality for Cosmos
//!
//! Provides version checking against crates.io and self-updating via
//! pre-built binaries from GitHub releases.

use anyhow::{Context, Result};
use serde::Deserialize;

/// Current version of Cosmos (from Cargo.toml)
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository owner
const REPO_OWNER: &str = "cameronspears";

/// GitHub repository name
const REPO_NAME: &str = "cosmos";

/// Binary name in releases
const BIN_NAME: &str = "cosmos";

/// Information about an available update
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
}

/// Response from crates.io API
#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    max_stable_version: String,
}

/// Check crates.io for the latest version
///
/// Returns `Some(UpdateInfo)` if a newer version is available, `None` if up to date.
pub async fn check_for_update() -> Result<Option<UpdateInfo>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("cosmos-tui/{}", CURRENT_VERSION))
        .build()
        .context("Failed to create HTTP client")?;

    let url = "https://crates.io/api/v1/crates/cosmos-tui";
    let response: CrateResponse = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch version info from crates.io")?
        .json()
        .await
        .context("Failed to parse crates.io response")?;

    let latest = &response.krate.max_stable_version;

    if is_newer_version(latest, CURRENT_VERSION) {
        Ok(Some(UpdateInfo {
            latest_version: latest.clone(),
        }))
    } else {
        Ok(None)
    }
}

/// Compare two semver version strings
/// Returns true if `latest` is newer than `current`
fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.trim_start_matches('v').split('.').collect();
        if parts.len() >= 3 {
            Some((
                parts[0].parse().ok()?,
                parts[1].parse().ok()?,
                parts[2].split('-').next()?.parse().ok()?,
            ))
        } else {
            None
        }
    };

    match (parse(latest), parse(current)) {
        (Some((l_major, l_minor, l_patch)), Some((c_major, c_minor, c_patch))) => {
            (l_major, l_minor, l_patch) > (c_major, c_minor, c_patch)
        }
        _ => false,
    }
}

/// Suppress stdout and stderr while running a closure.
///
/// The self_update crate prints status messages directly with println!
/// which corrupts the TUI display. This function temporarily redirects
/// stdout/stderr to /dev/null (or NUL on Windows) during the operation.
#[cfg(unix)]
fn suppress_output<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    // Open /dev/null
    let dev_null = match File::create("/dev/null") {
        Ok(f) => f,
        Err(_) => return f(), // Fall back to running unsuppressed
    };

    // Save original stdout/stderr
    let stdout_fd = std::io::stdout().as_raw_fd();
    let stderr_fd = std::io::stderr().as_raw_fd();

    // SAFETY: We're duplicating valid file descriptors
    let saved_stdout = unsafe { libc::dup(stdout_fd) };
    let saved_stderr = unsafe { libc::dup(stderr_fd) };

    if saved_stdout < 0 || saved_stderr < 0 {
        return f(); // Fall back to running unsuppressed
    }

    // Redirect stdout/stderr to /dev/null
    // SAFETY: We're redirecting to a valid file descriptor
    unsafe {
        libc::dup2(dev_null.as_raw_fd(), stdout_fd);
        libc::dup2(dev_null.as_raw_fd(), stderr_fd);
    }

    // Run the closure
    let result = f();

    // Restore original stdout/stderr
    // SAFETY: We're restoring valid saved file descriptors
    unsafe {
        libc::dup2(saved_stdout, stdout_fd);
        libc::dup2(saved_stderr, stderr_fd);
        libc::close(saved_stdout);
        libc::close(saved_stderr);
    }

    result
}

/// Fallback for non-Unix platforms - just run without suppression.
/// Windows terminals handle this differently and the issue is less pronounced.
#[cfg(not(unix))]
fn suppress_output<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    f()
}

/// Download and install the latest version from GitHub releases
///
/// This function downloads the appropriate binary for the current platform,
/// replaces the current executable, and then exec()s into the new binary.
///
/// On success, this function does not return (the process is replaced).
pub fn run_update<F>(target_version: &str, on_progress: F) -> Result<()>
where
    F: Fn(u8) + Send + 'static,
{
    use self_update::backends::github::Update;
    use self_update::update::UpdateStatus;

    // Initial progress
    on_progress(5);

    // Build the updater config (this doesn't print anything)
    let updater = Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .current_version(CURRENT_VERSION)
        .target_version_tag(&format!("v{}", target_version))
        .show_download_progress(false)
        .no_confirm(true)
        .build()
        .context("Failed to configure updater")?;

    // Run the update with stdout/stderr suppressed to prevent the self_update
    // crate's println! calls from corrupting the TUI display
    let status =
        suppress_output(|| updater.update_extended()).context("Failed to download update")?;

    // Update complete
    on_progress(100);

    match status {
        UpdateStatus::UpToDate => {
            // Already up to date - nothing to do
            Ok(())
        }
        UpdateStatus::Updated(release) => {
            // Binary was replaced, now exec into the new version
            exec_new_binary().map_err(|e| {
                anyhow::anyhow!(
                    "Update downloaded (v{}) but failed to restart: {}",
                    release.version,
                    e
                )
            })
        }
    }
}

/// Replace the current process with the new binary
///
/// On Unix, uses exec() to replace the process in-place.
/// On Windows, spawns the new process and exits.
fn exec_new_binary() -> Result<()> {
    let exe_path = std::env::current_exe().context("Failed to get current executable path")?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // Get the original arguments (skip the program name)
        let args: Vec<String> = std::env::args().skip(1).collect();

        // exec() replaces the current process - this never returns on success
        let err = std::process::Command::new(&exe_path).args(&args).exec();

        // If we get here, exec failed
        Err(anyhow::anyhow!("exec failed: {}", err))
    }

    #[cfg(windows)]
    {
        use std::process::Command;

        // Get the original arguments
        let args: Vec<String> = std::env::args().skip(1).collect();

        // Spawn the new process
        Command::new(&exe_path)
            .args(&args)
            .spawn()
            .context("Failed to spawn new process")?;

        // Exit the current process
        std::process::exit(0);
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(anyhow::anyhow!(
            "Self-update restart not supported on this platform"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison_basic() {
        // Newer versions
        assert!(is_newer_version("0.4.0", "0.3.0"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(is_newer_version("0.3.1", "0.3.0"));
        assert!(is_newer_version("0.3.10", "0.3.9"));
        assert!(is_newer_version("2.0.0", "1.99.99"));

        // Same version
        assert!(!is_newer_version("0.3.0", "0.3.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));

        // Older versions
        assert!(!is_newer_version("0.2.0", "0.3.0"));
        assert!(!is_newer_version("0.2.9", "0.3.0"));
        assert!(!is_newer_version("0.3.0", "0.3.1"));
    }

    #[test]
    fn test_version_comparison_with_v_prefix() {
        // v prefix should be handled
        assert!(is_newer_version("v0.4.0", "0.3.0"));
        assert!(is_newer_version("0.4.0", "v0.3.0"));
        assert!(is_newer_version("v0.4.0", "v0.3.0"));
        assert!(!is_newer_version("v0.3.0", "v0.3.0"));
    }

    #[test]
    fn test_version_comparison_prerelease() {
        // Prerelease suffixes should be stripped for comparison
        assert!(is_newer_version("0.4.0", "0.3.0-beta"));
        assert!(is_newer_version("0.4.0-alpha", "0.3.0"));
        assert!(!is_newer_version("0.3.0-beta", "0.3.0"));
    }

    #[test]
    fn test_version_comparison_invalid() {
        // Invalid versions should return false (safe default)
        assert!(!is_newer_version("invalid", "0.3.0"));
        assert!(!is_newer_version("0.3.0", "invalid"));
        assert!(!is_newer_version("", "0.3.0"));
        assert!(!is_newer_version("0.3.0", ""));
        assert!(!is_newer_version("1.0", "0.3.0")); // Only 2 parts
        assert!(!is_newer_version("0.3.0", "1.0")); // Only 2 parts
    }

    #[test]
    fn test_current_version_is_valid() {
        // Ensure CURRENT_VERSION can be parsed
        let parts: Vec<&str> = CURRENT_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "CURRENT_VERSION should have 3 parts");
        assert!(
            parts[0].parse::<u32>().is_ok(),
            "Major version should be numeric"
        );
        assert!(
            parts[1].parse::<u32>().is_ok(),
            "Minor version should be numeric"
        );
        // Patch may have prerelease suffix
        let patch = parts[2].split('-').next().unwrap();
        assert!(
            patch.parse::<u32>().is_ok(),
            "Patch version should be numeric"
        );
    }

    #[test]
    fn test_update_info_creation() {
        let info = UpdateInfo {
            latest_version: "0.4.0".to_string(),
        };
        assert_eq!(info.latest_version, "0.4.0");
    }

    #[test]
    fn test_version_comparison_major_bump() {
        // Major version bumps should always be newer
        assert!(is_newer_version("1.0.0", "0.99.99"));
        assert!(is_newer_version("2.0.0", "1.99.99"));
        assert!(is_newer_version("10.0.0", "9.99.99"));
    }

    #[test]
    fn test_version_comparison_minor_bump() {
        // Minor version bumps within same major
        assert!(is_newer_version("0.4.0", "0.3.99"));
        assert!(is_newer_version("1.2.0", "1.1.99"));
    }

    #[test]
    fn test_version_comparison_patch_bump() {
        // Patch version bumps within same minor
        assert!(is_newer_version("0.3.5", "0.3.4"));
        assert!(is_newer_version("0.3.100", "0.3.99"));
    }
}
