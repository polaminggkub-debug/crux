use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};

use super::types::FilterConfig;

/// Priority assigned to builtin filter stubs when no TOML config exists.
///
/// This is the lowest priority so that any user or stdlib TOML filter always
/// wins over the auto-generated builtin stub. The value -100 was chosen to
/// leave plenty of room for negative-priority TOML overrides while ensuring
/// builtins never accidentally shadow user config.
pub const BUILTIN_FALLBACK_PRIORITY: i32 = -100;

/// Directories searched for filter configs, in priority order:
/// 1. `.crux/filters/` — local project overrides
/// 2. `~/.config/crux/filters/` — global user filters
/// 3. Embedded stdlib (via `include_dir`)
///
/// First match wins. Most specific command match wins, then highest priority.
///
/// Resolve a filter for the given command tokens.
///
/// Returns `None` when no filter matches (passthrough behavior).
pub fn resolve_filter(command: &[String]) -> Option<FilterConfig> {
    if command.is_empty() {
        return None;
    }

    let mut candidates: Vec<FilterConfig> = Vec::new();

    // 1. Local project filters
    if let Ok(configs) = load_configs_from_dir(Path::new(".crux/filters")) {
        candidates.extend(configs);
    }

    // 2. Global user filters
    if let Some(home) = home_dir() {
        let global_dir = home.join(".config/crux/filters");
        if let Ok(configs) = load_configs_from_dir(&global_dir) {
            candidates.extend(configs);
        }
    }

    // 3. Embedded stdlib (cached after first parse)
    candidates.extend_from_slice(cached_embedded_stdlib());

    // 4. Builtin registry stubs (lowest priority fallback)
    // Ensures builtin handlers fire even when no TOML filters exist.
    for key in crate::filter::builtin::registry().keys() {
        if !candidates.iter().any(|c| c.command == *key) {
            candidates.push(FilterConfig {
                command: key.to_string(),
                priority: BUILTIN_FALLBACK_PRIORITY,
                ..Default::default()
            });
        }
    }

    // Try original command first
    if let Some(result) = find_best_match(&candidates, command) {
        return Some(result);
    }

    // Strip runner prefixes (npx, bunx, pnpx) and retry
    if command.len() >= 2 {
        let runner = command[0].as_str();
        if matches!(runner, "npx" | "bunx" | "pnpx") {
            return find_best_match(&candidates, &command[1..]);
        }
    }

    None
}

/// Build the full command string from tokens for matching.
fn command_string(command: &[String]) -> String {
    command.join(" ")
}

/// Score how well a filter's command pattern matches the input command.
///
/// Returns `None` if there is no match, or `Some(specificity)` where higher
/// values indicate a more specific match.
fn match_score(filter_command: &str, input_command: &str) -> Option<usize> {
    let filter_cmd = filter_command.trim();
    let input_cmd = input_command.trim();

    if input_cmd == filter_cmd {
        // Exact match — highest specificity = number of words
        return Some(filter_cmd.split_whitespace().count() * 100);
    }

    // Prefix match: "git" matches "git status", "git diff", etc.
    if input_cmd.starts_with(filter_cmd)
        && input_cmd[filter_cmd.len()..].starts_with(char::is_whitespace)
    {
        return Some(filter_cmd.split_whitespace().count() * 100);
    }

    None
}

/// Among all candidates, pick the best match for the given command.
fn find_best_match(candidates: &[FilterConfig], command: &[String]) -> Option<FilterConfig> {
    let input = command_string(command);

    let mut best: Option<(usize, i32, &FilterConfig)> = None;

    for config in candidates {
        if let Some(score) = match_score(&config.command, &input) {
            let dominated = match &best {
                Some((best_score, best_prio, _)) => {
                    score > *best_score || (score == *best_score && config.priority > *best_prio)
                }
                None => true,
            };
            if dominated {
                best = Some((score, config.priority, config));
            }
        }
    }

    best.map(|(_, _, config)| config.clone())
}

/// Recursively scan a directory for `.toml` files and parse them.
fn load_configs_from_dir(dir: &Path) -> Result<Vec<FilterConfig>> {
    let mut configs = Vec::new();
    if !dir.is_dir() {
        return Ok(configs);
    }
    collect_toml_files(dir, &mut configs)?;
    Ok(configs)
}

fn collect_toml_files(dir: &Path, configs: &mut Vec<FilterConfig>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Skip directories whose name ends with `_test` (declarative test suites).
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with("_test") {
                    continue;
                }
            }
            collect_toml_files(&path, configs)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            match parse_toml_file(&path) {
                Ok(config) => configs.push(config),
                Err(e) => {
                    eprintln!("crux: skipping {}: {e}", path.display());
                }
            }
        }
    }
    Ok(())
}

fn parse_toml_file(path: &Path) -> Result<FilterConfig> {
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let config: FilterConfig =
        toml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
    Ok(config)
}

/// Return a cached reference to parsed embedded stdlib filters.
///
/// The embedded TOML files are parsed once on first access and then reused
/// for every subsequent `resolve_filter` call, avoiding repeated
/// deserialization overhead on the hot path.
fn cached_embedded_stdlib() -> &'static [FilterConfig] {
    static CACHE: OnceLock<Vec<FilterConfig>> = OnceLock::new();
    CACHE.get_or_init(load_embedded_stdlib)
}

/// Load embedded stdlib filters compiled into the binary via `include_dir`.
fn load_embedded_stdlib() -> Vec<FilterConfig> {
    use include_dir::{include_dir, Dir};

    static STDLIB_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/filters");

    parse_embedded_dir(&STDLIB_DIR)
}

fn parse_embedded_dir(dir: &include_dir::Dir<'_>) -> Vec<FilterConfig> {
    let mut configs = Vec::new();

    for file in dir.files() {
        if file.path().extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(contents) = file.contents_utf8() {
                match toml::from_str::<FilterConfig>(contents) {
                    Ok(config) => configs.push(config),
                    Err(e) => {
                        eprintln!("crux: skipping embedded {}: {e}", file.path().display());
                    }
                }
            }
        }
    }

    for subdir in dir.dirs() {
        // Skip _test directories
        if let Some(name) = subdir.path().file_name().and_then(|n| n.to_str()) {
            if name.ends_with("_test") {
                continue;
            }
        }
        configs.extend(parse_embedded_dir(subdir));
    }

    configs
}

/// Counts of filters broken down by source category.
#[derive(Debug, Default)]
pub struct FilterCounts {
    pub builtin: usize,
    pub stdlib_toml: usize,
    pub user_local: usize,
    pub user_global: usize,
}

impl FilterCounts {
    pub fn total(&self) -> usize {
        self.builtin + self.stdlib_toml + self.user_local + self.user_global
    }
}

/// Count all available filters by source category.
pub fn count_filters() -> FilterCounts {
    let builtin = crate::filter::builtin::registry().len();
    let stdlib_toml = cached_embedded_stdlib().len();

    let user_local = load_configs_from_dir(Path::new(".crux/filters"))
        .map(|c| c.len())
        .unwrap_or(0);

    let user_global = home_dir()
        .and_then(|h| load_configs_from_dir(&h.join(".config/crux/filters")).ok())
        .map(|c| c.len())
        .unwrap_or(0);

    FilterCounts {
        builtin,
        stdlib_toml,
        user_local,
        user_global,
    }
}

/// Platform-aware home directory lookup.
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

    fn make_config(command: &str, priority: i32) -> FilterConfig {
        FilterConfig {
            command: command.to_string(),
            priority,
            ..Default::default()
        }
    }

    #[test]
    fn exact_match_wins_over_prefix() {
        let candidates = vec![make_config("git", 0), make_config("git status", 0)];
        let cmd = vec!["git".to_string(), "status".to_string()];
        let result = find_best_match(&candidates, &cmd).unwrap();
        assert_eq!(result.command, "git status");
    }

    #[test]
    fn prefix_match_works() {
        let candidates = vec![make_config("git", 0)];
        let cmd = vec!["git".to_string(), "log".to_string()];
        let result = find_best_match(&candidates, &cmd).unwrap();
        assert_eq!(result.command, "git");
    }

    #[test]
    fn no_match_returns_none() {
        let candidates = vec![make_config("cargo test", 0)];
        let cmd = vec!["git".to_string(), "status".to_string()];
        let result = find_best_match(&candidates, &cmd);
        assert!(result.is_none());
    }

    #[test]
    fn higher_priority_wins_when_same_specificity() {
        let candidates = vec![make_config("git status", 5), make_config("git status", 10)];
        let cmd = vec!["git".to_string(), "status".to_string()];
        let result = find_best_match(&candidates, &cmd).unwrap();
        assert_eq!(result.priority, 10);
    }

    #[test]
    fn empty_command_returns_none() {
        let result = resolve_filter(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn match_score_no_partial_word_match() {
        // "git" should NOT match "gitk"
        assert!(match_score("git", "gitk").is_none());
    }

    #[test]
    fn builtin_stubs_provide_fallback_match() {
        // Even with no TOML files, builtin commands should resolve
        let cmd = vec!["git".to_string(), "status".to_string()];
        let result = resolve_filter(&cmd);
        assert!(result.is_some(), "git status should match via builtin stub");
        assert_eq!(result.unwrap().command, "git status");
    }

    #[test]
    fn builtin_stubs_for_cargo_test() {
        let cmd = vec!["cargo".to_string(), "test".to_string()];
        let result = resolve_filter(&cmd);
        assert!(result.is_some(), "cargo test should match via builtin stub");
        assert_eq!(result.unwrap().command, "cargo test");
    }

    #[test]
    fn match_score_exact() {
        assert_eq!(match_score("git status", "git status"), Some(200));
    }

    #[test]
    fn match_score_prefix() {
        assert_eq!(match_score("git", "git status"), Some(100));
    }
}
