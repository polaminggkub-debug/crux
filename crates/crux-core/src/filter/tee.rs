use crate::config::types::TeeMode;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Save raw output to a tee file based on the tee mode setting.
/// Returns the path where the file was saved, or None if not saved.
pub fn maybe_save_tee(
    tee_mode: &TeeMode,
    command_slug: &str,
    raw_output: &str,
    exit_code: i32,
) -> Option<PathBuf> {
    let should_save = match tee_mode {
        TeeMode::Never => false,
        TeeMode::Failures => exit_code != 0,
        TeeMode::Always => true,
    };
    if !should_save {
        return None;
    }
    let dir = tee_dir()?;
    save_tee(&dir, command_slug, raw_output, 50)
}

fn save_tee(dir: &Path, command_slug: &str, raw_output: &str, max_files: usize) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let slug = sanitize_slug(command_slug);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let path = dir.join(format!("{slug}-{ts}.log"));
    std::fs::write(&path, raw_output).ok()?;
    rotate_tee_dir(dir, max_files);
    Some(path)
}

fn tee_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".local/share/crux/tee"))
}

fn sanitize_slug(s: &str) -> String {
    let sanitized: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    if sanitized.len() > 50 {
        sanitized[..50].to_string()
    } else {
        sanitized
    }
}

fn rotate_tee_dir(dir: &Path, max_files: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.sort();
    if files.len() > max_files {
        for f in &files[..files.len() - max_files] {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_mode_returns_none() {
        assert!(maybe_save_tee(&TeeMode::Never, "cmd", "out", 1).is_none());
    }

    #[test]
    fn failures_mode_saves_on_nonzero() {
        let dir = std::env::temp_dir().join("crux-tee-test-fail");
        let _ = std::fs::remove_dir_all(&dir);
        let path = save_tee(&dir, "cargo-test", "error output", 50);
        assert!(path.is_some());
        assert!(std::fs::read_to_string(path.unwrap())
            .unwrap()
            .contains("error output"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failures_mode_skips_on_zero() {
        assert!(maybe_save_tee(&TeeMode::Failures, "cmd", "ok", 0).is_none());
    }

    #[test]
    fn slug_sanitization() {
        assert_eq!(sanitize_slug("git status --short"), "git-status---short");
        assert_eq!(sanitize_slug(&"a".repeat(100)).len(), 50);
    }

    #[test]
    fn rotation_keeps_max_files() {
        let dir = std::env::temp_dir().join("crux-tee-test-rotate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("f-{i}.log")), "x").unwrap();
        }
        rotate_tee_dir(&dir, 3);
        let count = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
