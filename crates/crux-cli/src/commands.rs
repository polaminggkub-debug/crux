//! Subcommand implementations for crux CLI.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Ls — list available filters
// ---------------------------------------------------------------------------

pub fn cmd_ls() -> Result<()> {
    let mut entries = BTreeSet::new();

    for key in crux_core::filter::builtin::registry().keys() {
        entries.insert(format!("builtin: {key}"));
    }

    scan_toml_dir(Path::new(".crux/filters"), &mut entries);
    if let Some(home) = home_dir() {
        scan_toml_dir(&home.join(".config/crux/filters"), &mut entries);
    }

    if entries.is_empty() {
        println!("No filters found.");
    } else {
        for entry in &entries {
            println!("{entry}");
        }
    }
    Ok(())
}

fn scan_toml_dir(dir: &Path, entries: &mut BTreeSet<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_toml_dir(&path, entries);
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str::<crux_core::config::FilterConfig>(&contents) {
                    entries.insert(format!("toml: {}", config.command));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Show — display filter details
// ---------------------------------------------------------------------------

pub fn cmd_show(filter: &str) -> Result<()> {
    let tokens: Vec<String> = filter.split_whitespace().map(String::from).collect();
    let config = crux_core::config::resolve_filter(&tokens)
        .with_context(|| format!("no filter matches '{filter}'"))?;

    println!("Command:     {}", config.command);
    if let Some(desc) = &config.description {
        println!("Description: {desc}");
    }
    println!("Priority:    {}", config.priority);
    println!(
        "Builtin:     {}",
        crux_core::filter::builtin::registry().contains_key(config.command.as_str())
    );
    if !config.skip.is_empty() {
        println!("Skip:        {:?}", config.skip);
    }
    if !config.keep.is_empty() {
        println!("Keep:        {:?}", config.keep);
    }
    if !config.replace.is_empty() {
        println!("Replace rules: {}", config.replace.len());
        for r in &config.replace {
            println!("  /{}/  →  {}", r.pattern, r.replacement);
        }
    }
    if !config.section.is_empty() {
        println!("Section rules: {}", config.section.len());
    }
    if !config.extract.is_empty() {
        println!("Extract rules: {}", config.extract.len());
    }
    if config.dedup == Some(true) {
        println!("Dedup:       true");
    }
    if config.strip_ansi == Some(true) {
        println!("Strip ANSI:  true");
    }
    if config.collapse_blank_lines == Some(true) {
        println!("Collapse blanks: true");
    }
    if config.trim_trailing_whitespace == Some(true) {
        println!("Trim trailing: true");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Eject — export filter as TOML
// ---------------------------------------------------------------------------

pub fn cmd_eject(filter: &str) -> Result<()> {
    let tokens: Vec<String> = filter.split_whitespace().map(String::from).collect();
    let config = crux_core::config::resolve_filter(&tokens)
        .with_context(|| format!("no filter matches '{filter}'"))?;

    let toml_str =
        toml::to_string_pretty(&config).context("failed to serialize filter config to TOML")?;
    println!("# Ejected filter for: {}", config.command);
    println!(
        "# Save to .crux/filters/{}.toml to customize",
        filter.replace(' ', "-")
    );
    println!();
    print!("{toml_str}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Verify — run declarative tests
// ---------------------------------------------------------------------------

pub fn cmd_verify() -> Result<()> {
    let mut total = 0;
    let mut passed = 0;

    // 1. Embedded stdlib test suites (compiled into the binary)
    let embedded = crux_core::verify::verify_embedded_stdlib();
    for tr in &embedded.results {
        total += 1;
        if tr.passed {
            passed += 1;
            println!("  PASS  {}", tr.name);
        } else {
            println!("  FAIL  {}", tr.name);
            print_diff(&tr.expected, &tr.actual);
        }
    }

    // 2. Filesystem test suites (local + global)
    verify_dir(Path::new(".crux/filters"), &mut total, &mut passed)?;
    if let Some(home) = home_dir() {
        verify_dir(&home.join(".config/crux/filters"), &mut total, &mut passed)?;
    }

    if total == 0 {
        println!("No test cases found. Add _test/ directories next to filter TOMLs.");
        println!("Each _test/ dir should contain input.txt/expected.txt or <name>.input/<name>.expected pairs.");
    } else {
        println!("\n{passed}/{total} tests passed");
        if passed < total {
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Print a unified-style diff between expected and actual output.
fn print_diff(expected: &str, actual: &str) {
    let expected_lines: Vec<&str> = expected.trim().lines().collect();
    let actual_lines: Vec<&str> = actual.trim().lines().collect();
    let max_lines = expected_lines.len().max(actual_lines.len());
    for i in 0..max_lines {
        let exp = expected_lines.get(i).unwrap_or(&"");
        let act = actual_lines.get(i).unwrap_or(&"");
        if exp != act {
            println!("    - {exp}");
            println!("    + {act}");
        }
    }
}

fn verify_dir(dir: &Path, total: &mut usize, passed: &mut usize) -> Result<()> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with("_test") {
                let base_name = name.strip_suffix("_test").unwrap_or(name);
                let toml_path = dir.join(format!("{base_name}.toml"));
                if toml_path.exists() {
                    run_test_suite(&toml_path, &path, total, passed)?;
                }
            } else {
                verify_dir(&path, total, passed)?;
            }
        }
    }
    Ok(())
}

fn run_test_suite(
    toml_path: &Path,
    test_dir: &Path,
    total: &mut usize,
    passed: &mut usize,
) -> Result<()> {
    let contents = std::fs::read_to_string(toml_path)?;
    let config: crux_core::config::FilterConfig = toml::from_str(&contents)?;

    // Check for input.txt / expected.txt pair (single test case)
    let input_txt = test_dir.join("input.txt");
    let expected_txt = test_dir.join("expected.txt");
    if input_txt.exists() && expected_txt.exists() {
        *total += 1;
        let input = std::fs::read_to_string(&input_txt)?;
        let expected = std::fs::read_to_string(&expected_txt)?;
        let actual = crux_core::filter::apply_filter(&config, &input, 0);

        let test_name = format!("{}::default", config.command);
        if actual.trim() == expected.trim() {
            *passed += 1;
            println!("  PASS  {test_name}");
        } else {
            println!("  FAIL  {test_name}");
            print_diff(&expected, &actual);
        }
    }

    // Check for <name>.input / <name>.expected pairs
    let Ok(rd) = std::fs::read_dir(test_dir) else {
        return Ok(());
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("input") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let expected_path = test_dir.join(format!("{stem}.expected"));
            if !expected_path.exists() {
                continue;
            }
            *total += 1;
            let input = std::fs::read_to_string(&path)?;
            let expected = std::fs::read_to_string(&expected_path)?;
            let actual = crux_core::filter::apply_filter(&config, &input, 0);

            let test_name = format!("{}::{stem}", config.command);
            if actual.trim() == expected.trim() {
                *passed += 1;
                println!("  PASS  {test_name}");
            } else {
                println!("  FAIL  {test_name}");
                print_diff(&expected, &actual);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Init — install Claude Code hook
// ---------------------------------------------------------------------------

pub fn cmd_init(global: bool, codex: bool) -> Result<()> {
    if codex {
        return crux_hook::codex::install_codex_skill();
    }

    let settings_path = if global {
        home_dir()
            .context("cannot determine home directory")?
            .join(".claude/settings.json")
    } else {
        PathBuf::from(".claude/settings.json")
    };

    let hook_value = serde_json::json!({
        "hooks": {
            "command_output": "crux run"
        }
    });

    let merged = if settings_path.exists() {
        let contents =
            std::fs::read_to_string(&settings_path).context("reading existing settings.json")?;
        let mut existing: serde_json::Value =
            serde_json::from_str(&contents).context("parsing settings.json")?;
        if let Some(obj) = existing.as_object_mut() {
            if let Some(hooks_val) = hook_value.get("hooks") {
                obj.insert("hooks".to_string(), hooks_val.clone());
            }
        }
        existing
    } else {
        hook_value
    };

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json_str = serde_json::to_string_pretty(&merged)?;
    std::fs::write(&settings_path, json_str)?;

    let scope = if global { "global" } else { "local" };
    println!(
        "crux: installed Claude Code hook ({scope}): {}",
        settings_path.display()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Err — error-only filter
// ---------------------------------------------------------------------------

pub fn cmd_err(command: &[String]) -> Result<()> {
    let result = crux_core::runner::run_command(command)?;
    let re = regex::Regex::new(
        r"(?im)^.*(error[:\[]|fatal[:\s]|panic[:\s]|exception[:\s]|traceback|fail(ed|ure)?[:\s]).*$",
    )?;

    let filtered: Vec<&str> = result
        .combined
        .lines()
        .filter(|line| re.is_match(line))
        .collect();

    if filtered.is_empty() {
        println!("(no error lines detected)");
    } else {
        for line in &filtered {
            println!("{line}");
        }
    }

    if result.exit_code != 0 {
        eprintln!("crux: exit code {}", result.exit_code);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Test — test summary filter
// ---------------------------------------------------------------------------

pub fn cmd_test(command: &[String]) -> Result<()> {
    let result = crux_core::runner::run_command(command)?;
    let output = &result.combined;
    let registry = crux_core::filter::builtin::registry();

    let framework_keys = [
        "cargo test",
        "npm test",
        "pytest",
        "go test",
        "jest",
        "vitest",
    ];

    for key in &framework_keys {
        if let Some(handler) = registry.get(key) {
            let looks_like = match *key {
                "cargo test" => output.contains("test result:") || output.contains("running"),
                "pytest" => output.contains("passed") && output.contains("=="),
                "go test" => output.contains("--- PASS") || output.contains("--- FAIL"),
                "jest" | "vitest" => output.contains("Tests:") || output.contains("Test Suites:"),
                "npm test" => output.contains("npm test"),
                _ => false,
            };
            if looks_like {
                let filtered = handler(output, result.exit_code);
                print!("{filtered}");
                if !filtered.ends_with('\n') && !filtered.is_empty() {
                    println!();
                }
                return Ok(());
            }
        }
    }

    // Fallback: show last 10 lines
    let lines: Vec<&str> = output.lines().collect();
    let start = lines.len().saturating_sub(10);
    for line in &lines[start..] {
        println!("{line}");
    }

    if result.exit_code != 0 {
        eprintln!("crux: exit code {}", result.exit_code);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Log — dedup + collapse filter
// ---------------------------------------------------------------------------

pub fn cmd_log(command: &[String]) -> Result<()> {
    let result = crux_core::runner::run_command(command)?;

    let config = crux_core::config::FilterConfig {
        command: command.join(" "),
        builtin: Some(false),
        dedup: Some(true),
        collapse_blank_lines: Some(true),
        trim_trailing_whitespace: Some(true),
        ..Default::default()
    };

    let filtered = crux_core::filter::apply_filter(&config, &result.combined, result.exit_code);
    print!("{filtered}");
    if !filtered.ends_with('\n') && !filtered.is_empty() {
        println!();
    }

    if result.exit_code != 0 {
        eprintln!("crux: exit code {}", result.exit_code);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
