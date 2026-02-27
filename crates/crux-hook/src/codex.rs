//! Codex hook integration for crux.
//!
//! Codex uses a configuration-based approach. Since the exact hook format may
//! vary, we create a wrapper script at `~/.local/bin/crux-codex-wrapper` that
//! pipes commands through `crux run`, and print setup instructions for the user.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The wrapper script content that intercepts commands and routes them through crux.
const WRAPPER_SCRIPT: &str = r#"#!/usr/bin/env bash
# crux-codex-wrapper — wraps shell commands through crux for token compression.
# Installed by: crux init --codex
#
# Usage: crux-codex-wrapper <command> [args...]
#
# If crux is available and the command is supported, output is compressed.
# Otherwise, the command runs normally as a passthrough.

set -euo pipefail

if ! command -v crux &>/dev/null; then
    exec "$@"
fi

exec crux run "$@"
"#;

/// Directory under $HOME where the wrapper is installed.
const WRAPPER_DIR: &str = ".local/bin";

/// Filename for the wrapper script.
const WRAPPER_NAME: &str = "crux-codex-wrapper";

/// Install the Codex integration for crux.
///
/// This creates a wrapper script and prints configuration instructions
/// for the user to wire it into their Codex setup.
pub fn install_codex_skill() -> Result<()> {
    let wrapper_path = install_wrapper_script()?;

    print_setup_instructions(&wrapper_path);

    Ok(())
}

/// Create the wrapper script at `~/.local/bin/crux-codex-wrapper`.
///
/// Returns the absolute path to the installed script.
fn install_wrapper_script() -> Result<PathBuf> {
    let home = home_dir().context("cannot determine home directory")?;
    let dir = home.join(WRAPPER_DIR);
    let wrapper_path = dir.join(WRAPPER_NAME);

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create directory: {}", dir.display()))?;

    std::fs::write(&wrapper_path, WRAPPER_SCRIPT)
        .with_context(|| format!("failed to write wrapper script: {}", wrapper_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&wrapper_path, perms).with_context(|| {
            format!(
                "failed to set executable permissions: {}",
                wrapper_path.display()
            )
        })?;
    }

    Ok(wrapper_path)
}

/// Print human-readable setup instructions to stdout.
fn print_setup_instructions(wrapper_path: &Path) {
    println!(
        "crux: installed Codex wrapper script: {}",
        wrapper_path.display()
    );
    println!();
    println!("To configure Codex to use crux, add the following to your");
    println!("Codex config file (~/.codex/config.json or codex.json):");
    println!();
    println!("  {{");
    println!("    \"shell\": \"{}\"", wrapper_path.display());
    println!("  }}");
    println!();
    println!("Or, if Codex supports a command hook, set:");
    println!();
    println!("  {{");
    println!("    \"hooks\": {{");
    println!("      \"command_wrapper\": \"{}\"", wrapper_path.display());
    println!("    }}");
    println!("  }}");
    println!();
    println!(
        "Make sure {} is in your PATH.",
        wrapper_path.parent().unwrap().display()
    );
}

/// Build the wrapper script content for a given crux binary path.
///
/// This is used in testing to verify the script content without
/// actually installing to the filesystem.
pub fn build_wrapper_script() -> &'static str {
    WRAPPER_SCRIPT
}

/// Resolve the expected wrapper path without installing.
pub fn wrapper_path() -> Result<PathBuf> {
    let home = home_dir().context("cannot determine home directory")?;
    Ok(home.join(WRAPPER_DIR).join(WRAPPER_NAME))
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_script_is_valid_bash() {
        let script = build_wrapper_script();
        assert!(script.starts_with("#!/usr/bin/env bash"));
        assert!(script.contains("crux run"));
        assert!(script.contains("exec \"$@\""));
    }

    #[test]
    fn wrapper_script_has_passthrough_fallback() {
        // If crux is not available, the script should fall through to exec "$@"
        let script = build_wrapper_script();
        assert!(script.contains("command -v crux"));
        assert!(
            script.contains("exec \"$@\""),
            "script must have passthrough for when crux is not installed"
        );
    }

    #[test]
    fn wrapper_path_uses_home_dir() {
        // Temporarily override HOME for this test
        let original = std::env::var("HOME").ok();
        std::env::set_var("HOME", "/tmp/crux-test-home");

        let path = wrapper_path().unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/crux-test-home/.local/bin/crux-codex-wrapper")
        );

        // Restore
        if let Some(val) = original {
            std::env::set_var("HOME", val);
        }
    }

    #[test]
    fn install_creates_executable_script() {
        // Use a temp dir as HOME to avoid polluting the real filesystem
        let tmp = std::env::temp_dir().join("crux-codex-test-install");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let original = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.to_str().unwrap());

        let result = install_wrapper_script();
        assert!(result.is_ok(), "install_wrapper_script should succeed");

        let path = result.unwrap();
        assert!(path.exists(), "wrapper script should exist on disk");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("crux run"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "wrapper script should be executable");
        }

        // Cleanup
        if let Some(val) = original {
            std::env::set_var("HOME", val);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn install_is_idempotent() {
        let tmp = std::env::temp_dir().join("crux-codex-test-idempotent");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let original = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.to_str().unwrap());

        // Install twice — should not fail
        let r1 = install_wrapper_script();
        assert!(r1.is_ok());
        let r2 = install_wrapper_script();
        assert!(r2.is_ok());

        let path = r2.unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("crux run"));

        if let Some(val) = original {
            std::env::set_var("HOME", val);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
