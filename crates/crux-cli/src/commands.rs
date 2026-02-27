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

    scan_toml_dir(Path::new(".crux/filters"), "toml/local", &mut entries);
    if let Some(home) = home_dir() {
        scan_toml_dir(
            &home.join(".config/crux/filters"),
            "toml/global",
            &mut entries,
        );
    }

    // Embedded stdlib TOML filters
    let stdlib_configs = crux_core::config::count_filters();
    // We already added builtins above; now scan embedded stdlib for listing
    for config in load_embedded_stdlib_names() {
        entries.insert(format!("toml/stdlib: {config}"));
    }

    if entries.is_empty() {
        println!("No filters found.");
    } else {
        for entry in &entries {
            println!("{entry}");
        }
        println!();
        println!(
            "{} builtin filters, {} TOML stdlib filters, {} user filters",
            stdlib_configs.builtin,
            stdlib_configs.stdlib_toml,
            stdlib_configs.user_local + stdlib_configs.user_global,
        );
    }
    Ok(())
}

/// Collect command names from the embedded stdlib TOML filters.
fn load_embedded_stdlib_names() -> Vec<String> {
    use include_dir::{include_dir, Dir};

    static STDLIB_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../crux-core/filters");

    collect_embedded_names(&STDLIB_DIR)
}

fn collect_embedded_names(dir: &include_dir::Dir<'_>) -> Vec<String> {
    let mut names = Vec::new();
    for file in dir.files() {
        if file.path().extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(contents) = file.contents_utf8() {
                if let Ok(config) = toml::from_str::<crux_core::config::FilterConfig>(contents) {
                    names.push(config.command);
                }
            }
        }
    }
    for subdir in dir.dirs() {
        if let Some(name) = subdir.path().file_name().and_then(|n| n.to_str()) {
            if name.ends_with("_test") {
                continue;
            }
        }
        names.extend(collect_embedded_names(subdir));
    }
    names
}

fn scan_toml_dir(dir: &Path, label: &str, entries: &mut BTreeSet<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_toml_dir(&path, label, entries);
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str::<crux_core::config::FilterConfig>(&contents) {
                    entries.insert(format!("{label}: {}", config.command));
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
    let config = crux_core::config::resolve_filter(&tokens).with_context(|| {
        format!("no filter matches '{filter}'. Run `crux ls` to see all available filters")
    })?;

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
    let config = crux_core::config::resolve_filter(&tokens).with_context(|| {
        format!("no filter matches '{filter}'. Run `crux ls` to see all available filters")
    })?;

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

/// Detect which test framework produced the given output.
/// Returns `None` when no framework signature is recognized.
fn detect_framework(output: &str) -> Option<&'static str> {
    // cargo test: require "test result:" with ok/FAILED, or "running N test"
    if output.contains("test result: ok")
        || output.contains("test result: FAILED")
        || (output.contains("running") && output.contains("test"))
    {
        return Some("cargo test");
    }

    // pytest: require `=====` separator AND one of the key result words
    if output.contains("=====")
        && (output.contains("passed")
            || output.contains("failed")
            || output.contains("error")
            || output.contains("warnings summary"))
    {
        return Some("pytest");
    }

    // go test: "--- PASS" or "--- FAIL" (go-specific format)
    if output.contains("--- PASS") || output.contains("--- FAIL") {
        return Some("go test");
    }

    // jest: "Test Suites:" is jest-specific
    if output.contains("Test Suites:") {
        return Some("jest");
    }
    // jest per-file lines (PASS /FAIL at start of line) with summary
    let has_per_file = output
        .lines()
        .any(|l| l.trim_start().starts_with("PASS ") || l.trim_start().starts_with("FAIL "));
    if has_per_file && (output.contains("Tests:") || output.contains("Time:")) {
        return Some("jest");
    }

    // vitest: "Tests  N" (two spaces) with "Duration "
    if output.contains("Duration ") && output.contains("Tests ") {
        return Some("vitest");
    }
    if has_per_file && output.lines().any(|l| l.trim().starts_with("Tests ")) {
        return Some("vitest");
    }

    // mocha: "N passing" with timing like "(123ms)" or "(2s)"
    let mocha_re = regex::Regex::new(r"\d+\s+passing\s+\(\d+\w*s?\)").unwrap();
    if mocha_re.is_match(output) {
        return Some("mocha");
    }

    // playwright: two or more lines matching "N passed/failed/skipped"
    let pw_re = regex::Regex::new(r"\d+\s+(passed|failed|skipped)").unwrap();
    let pw_hits = output.lines().filter(|l| pw_re.is_match(l)).count();
    if pw_hits >= 2 {
        return Some("playwright");
    }

    // rspec: "N example(s), N failure(s)"
    let rspec_re = regex::Regex::new(r"\d+\s+examples?,\s+\d+\s+failures?").unwrap();
    if rspec_re.is_match(output) {
        return Some("rspec");
    }

    // PHPUnit: "OK (N tests, N assertions)" or "FAILURES!" with test counts
    let phpunit_ok_re = regex::Regex::new(r"OK\s+\(\d+\s+tests?,\s+\d+\s+assertions?\)").unwrap();
    if phpunit_ok_re.is_match(output) {
        return Some("phpunit");
    }
    if output.contains("FAILURES!") {
        let phpunit_summary_re = regex::Regex::new(r"Tests:\s+\d+.*Assertions:\s+\d+").unwrap();
        if phpunit_summary_re.is_match(output) {
            return Some("phpunit");
        }
    }

    // dotnet test: "Passed!" or "Failed!" with "Total tests:"
    if output.contains("Total tests:") && (output.contains("Passed!") || output.contains("Failed!"))
    {
        return Some("dotnet test");
    }

    // npm test: very low priority — only literal "npm test" string
    if output.contains("npm test") {
        return Some("npm test");
    }

    None
}

/// Extract lines containing test-related keywords (case-insensitive).
/// Falls back to last 10 lines when nothing matches.
fn fallback_extract(output: &str) -> String {
    let keyword_re = regex::Regex::new(r"(?i)(pass|fail|error|warning)").unwrap();
    let relevant: Vec<&str> = output
        .lines()
        .filter(|line| keyword_re.is_match(line))
        .collect();

    if relevant.is_empty() {
        let lines: Vec<&str> = output.lines().collect();
        let start = lines.len().saturating_sub(10);
        lines[start..].join("\n")
    } else {
        relevant.join("\n")
    }
}

// -- generic filters for frameworks without a dedicated builtin handler ------

fn generic_framework_filter(output: &str, exit_code: i32, framework: &str) -> String {
    match framework {
        "mocha" => filter_mocha(output, exit_code),
        "playwright" => filter_playwright(output, exit_code),
        "rspec" => filter_rspec(output, exit_code),
        "phpunit" => filter_phpunit(output, exit_code),
        "dotnet test" => filter_dotnet_test(output, exit_code),
        _ => fallback_extract(output),
    }
}

fn filter_mocha(output: &str, exit_code: i32) -> String {
    let passing_re = regex::Regex::new(r"^\s*\d+\s+passing").unwrap();
    let failing_re = regex::Regex::new(r"^\s*\d+\s+failing").unwrap();
    let pending_re = regex::Regex::new(r"^\s*\d+\s+pending").unwrap();
    let error_re =
        regex::Regex::new(r"(?i)(AssertionError|AssertError|Error:|expected|actual)").unwrap();

    let mut summary = Vec::new();
    let mut failures = Vec::new();

    for line in output.lines() {
        let t = line.trim();
        if passing_re.is_match(t) || failing_re.is_match(t) || pending_re.is_match(t) {
            summary.push(t.to_string());
        } else if exit_code != 0 && error_re.is_match(t) {
            failures.push(format!("  {t}"));
        }
    }

    build_test_output(&summary, &failures, exit_code)
}

fn filter_playwright(output: &str, exit_code: i32) -> String {
    let count_re = regex::Regex::new(r"^\s*\d+\s+(passed|failed|skipped|flaky)").unwrap();
    let numbered_re = regex::Regex::new(r"^\s*\d+\)").unwrap();

    let mut summary = Vec::new();
    let mut failures = Vec::new();
    let mut in_error = false;

    for line in output.lines() {
        let t = line.trim();
        if count_re.is_match(t) {
            summary.push(t.to_string());
            in_error = false;
        } else if numbered_re.is_match(t) {
            in_error = true;
            failures.push(t.to_string());
        } else if in_error
            && !t.is_empty()
            && (t.contains("Error:")
                || t.contains("expect(")
                || t.contains("Received")
                || t.contains("Expected"))
        {
            failures.push(format!("  {t}"));
        }
    }

    build_test_output(&summary, &failures, exit_code)
}

fn filter_rspec(output: &str, exit_code: i32) -> String {
    let summary_re = regex::Regex::new(r"\d+\s+examples?,\s+\d+\s+failures?").unwrap();
    let failure_re = regex::Regex::new(r"^\s*\d+\)\s+").unwrap();

    let mut summary = Vec::new();
    let mut failures = Vec::new();

    for line in output.lines() {
        let t = line.trim();
        if summary_re.is_match(t) {
            summary.push(t.to_string());
        } else if exit_code != 0 && failure_re.is_match(t) {
            failures.push(format!("  {t}"));
        }
    }

    build_test_output(&summary, &failures, exit_code)
}

fn filter_phpunit(output: &str, exit_code: i32) -> String {
    let ok_re = regex::Regex::new(r"OK\s+\(\d+\s+tests?,\s+\d+\s+assertions?\)").unwrap();
    let counts_re = regex::Regex::new(r"Tests:\s+\d+.*Assertions:\s+\d+").unwrap();
    let numbered_re = regex::Regex::new(r"^\s*\d+\)\s+").unwrap();

    let mut summary = Vec::new();
    let mut failures = Vec::new();

    for line in output.lines() {
        let t = line.trim();
        if ok_re.is_match(t) || counts_re.is_match(t) || t == "FAILURES!" {
            summary.push(t.to_string());
        } else if exit_code != 0 && numbered_re.is_match(t) {
            failures.push(format!("  {t}"));
        }
    }

    build_test_output(&summary, &failures, exit_code)
}

fn filter_dotnet_test(output: &str, exit_code: i32) -> String {
    let total_re = regex::Regex::new(r"Total tests:\s+\d+").unwrap();
    let failed_detail_re = regex::Regex::new(r"(?i)^\s*Failed\s+\w").unwrap();

    let mut summary = Vec::new();
    let mut failures = Vec::new();

    for line in output.lines() {
        let t = line.trim();
        if t.starts_with("Passed!") || t.starts_with("Failed!") || total_re.is_match(t) {
            summary.push(t.to_string());
        } else if exit_code != 0 && failed_detail_re.is_match(t) {
            failures.push(format!("  {t}"));
        }
    }

    build_test_output(&summary, &failures, exit_code)
}

/// Shared helper: compose "Failures:" block + summary lines.
fn build_test_output(summary: &[String], failures: &[String], exit_code: i32) -> String {
    let mut parts = Vec::new();

    if exit_code != 0 && !failures.is_empty() {
        parts.push("Failures:".to_string());
        parts.extend(failures.iter().cloned());
        parts.push(String::new());
    }

    if !summary.is_empty() {
        parts.extend(summary.iter().cloned());
    } else if exit_code == 0 {
        parts.push("All tests passed.".to_string());
    } else {
        parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    parts.join("\n")
}

pub fn cmd_test(command: &[String]) -> Result<()> {
    let result = crux_core::runner::run_command(command)?;
    let output = &result.combined;
    let registry = crux_core::filter::builtin::registry();

    if let Some(framework) = detect_framework(output) {
        // Try the builtin handler first
        if let Some(handler) = registry.get(framework) {
            let filtered = handler(output, result.exit_code);
            print!("{filtered}");
            if !filtered.ends_with('\n') && !filtered.is_empty() {
                println!();
            }
            return Ok(());
        }

        // No builtin handler — use generic framework filter
        let filtered = generic_framework_filter(output, result.exit_code, framework);
        print!("{filtered}");
        if !filtered.ends_with('\n') && !filtered.is_empty() {
            println!();
        }
        return Ok(());
    }

    // No framework detected — smart fallback
    let filtered = fallback_extract(output);
    println!("{filtered}");

    if result.exit_code != 0 {
        eprintln!("crux: exit code {}", result.exit_code);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests for framework detection and filters
// ---------------------------------------------------------------------------

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
// Doctor — diagnostic health check
// ---------------------------------------------------------------------------

pub fn cmd_doctor() -> Result<()> {
    println!("crux doctor");
    println!("===========\n");

    // Version info
    println!("Version:  {}", crux_core::VERSION);
    println!(
        "Tracking: {}",
        if cfg!(feature = "tracking") {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!();

    // Is crux on PATH?
    let on_path = std::process::Command::new("which")
        .arg("crux")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    print_check("crux on PATH", on_path, "add crux to your PATH");

    // Is Claude Code hook installed?
    let hook_installed = home_dir()
        .map(|h| {
            let settings = h.join(".claude/settings.json");
            if settings.exists() {
                std::fs::read_to_string(&settings)
                    .map(|c| c.contains("crux"))
                    .unwrap_or(false)
            } else {
                false
            }
        })
        .unwrap_or(false);
    print_check(
        "Claude Code hook installed",
        hook_installed,
        "run `crux init --global` to install",
    );

    // Filter counts
    let counts = crux_core::config::count_filters();
    let has_filters = counts.total() > 0;
    print_check(
        &format!(
            "Filters available ({} builtin, {} stdlib, {} user)",
            counts.builtin,
            counts.stdlib_toml,
            counts.user_local + counts.user_global
        ),
        has_filters,
        "something is wrong with the installation",
    );

    // Tracking database
    #[cfg(feature = "tracking")]
    {
        let db_ok = crux_tracking::db::default_db_path()
            .and_then(|p| crux_tracking::db::open_db(&p).map(|_| ()))
            .is_ok();
        print_check(
            "Tracking database accessible",
            db_ok,
            "check ~/.local/share/crux/ permissions",
        );
    }

    #[cfg(not(feature = "tracking"))]
    {
        println!("  [--] Tracking database (feature disabled)");
    }

    println!();
    if on_path && hook_installed && has_filters {
        println!("All checks passed.");
    } else {
        println!("Some checks failed. See suggestions above.");
    }

    Ok(())
}

fn print_check(label: &str, ok: bool, hint: &str) {
    if ok {
        println!("  [ok] {label}");
    } else {
        println!("  [!!] {label}");
        println!("       hint: {hint}");
    }
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
#[cfg(test)]
mod test_detection {
    use super::*;

    // -- cargo test --

    #[test]
    fn detect_cargo_test_ok() {
        let output = "running 5 tests\ntest foo ... ok\ntest result: ok. 5 passed; 0 failed;";
        assert_eq!(detect_framework(output), Some("cargo test"));
    }

    #[test]
    fn detect_cargo_test_failed() {
        let output =
            "running 3 tests\ntest bar ... FAILED\ntest result: FAILED. 1 passed; 2 failed;";
        assert_eq!(detect_framework(output), Some("cargo test"));
    }

    // -- pytest --

    #[test]
    fn detect_pytest_passed() {
        let output = "============================= test session starts ========\n\
                       ============================== 5 passed in 0.12s ========";
        assert_eq!(detect_framework(output), Some("pytest"));
    }

    #[test]
    fn detect_pytest_failed() {
        let output = "============================= test session starts ========\n\
                       =============== 1 failed, 2 passed in 0.15s =============";
        assert_eq!(detect_framework(output), Some("pytest"));
    }

    #[test]
    fn detect_pytest_warnings_summary() {
        let output = "============================= warnings summary ============\n\
                       ============================== 3 passed in 0.10s ========";
        assert_eq!(detect_framework(output), Some("pytest"));
    }

    #[test]
    fn no_false_positive_pytest() {
        // "passed" + "==" without "=====" should NOT match
        let output = "Build passed\nresult == expected\nDone.";
        assert_ne!(detect_framework(output), Some("pytest"));
    }

    // -- go test --

    #[test]
    fn detect_go_test() {
        let output = "=== RUN TestAdd\n--- PASS: TestAdd (0.00s)\nok example.com/math 0.003s";
        assert_eq!(detect_framework(output), Some("go test"));
    }

    // -- jest --

    #[test]
    fn detect_jest_suites() {
        let output = "Test Suites:  1 passed, 1 total\nTests:  2 passed\nTime:  0.9 s";
        assert_eq!(detect_framework(output), Some("jest"));
    }

    #[test]
    fn detect_jest_per_file_pass_fail() {
        let output = "PASS src/a.test.js\nFAIL src/b.test.js\nTests: 2 total\nTime: 1s";
        assert_eq!(detect_framework(output), Some("jest"));
    }

    // -- vitest --

    #[test]
    fn detect_vitest() {
        let output = " PASS  src/utils.test.ts\n Tests  6 passed (6)\n Duration  1.23s";
        assert_eq!(detect_framework(output), Some("vitest"));
    }

    // -- mocha --

    #[test]
    fn detect_mocha() {
        let output = "  3 passing (45ms)\n  1 failing";
        assert_eq!(detect_framework(output), Some("mocha"));
    }

    #[test]
    fn detect_mocha_seconds() {
        let output = "  12 passing (2s)";
        assert_eq!(detect_framework(output), Some("mocha"));
    }

    // -- playwright --

    #[test]
    fn detect_playwright() {
        let output = "Running 5 tests\n\n  5 passed (3s)\n  0 failed\n  1 skipped";
        assert_eq!(detect_framework(output), Some("playwright"));
    }

    // -- rspec --

    #[test]
    fn detect_rspec() {
        let output = "Finished in 0.5 seconds\n3 examples, 0 failures";
        assert_eq!(detect_framework(output), Some("rspec"));
    }

    #[test]
    fn detect_rspec_with_failures() {
        let output = "Finished in 1.2 seconds\n5 examples, 2 failures";
        assert_eq!(detect_framework(output), Some("rspec"));
    }

    // -- PHPUnit --

    #[test]
    fn detect_phpunit_ok() {
        let output = "PHPUnit 10.0.0\n...\nOK (5 tests, 10 assertions)";
        assert_eq!(detect_framework(output), Some("phpunit"));
    }

    #[test]
    fn detect_phpunit_failures() {
        let output = "PHPUnit 10.0.0\nFAILURES!\nTests: 5, Assertions: 10, Failures: 2";
        assert_eq!(detect_framework(output), Some("phpunit"));
    }

    // -- dotnet test --

    #[test]
    fn detect_dotnet_test_passed() {
        let output = "Passed! - Failed: 0, Passed: 5\nTotal tests: 5";
        assert_eq!(detect_framework(output), Some("dotnet test"));
    }

    #[test]
    fn detect_dotnet_test_failed() {
        let output = "Failed! - Failed: 2, Passed: 3\nTotal tests: 5";
        assert_eq!(detect_framework(output), Some("dotnet test"));
    }

    // -- no match --

    #[test]
    fn detect_none_for_generic_output() {
        let output = "Hello world\nSome output\nDone.";
        assert_eq!(detect_framework(output), None);
    }

    // -- fallback --

    #[test]
    fn fallback_extracts_keyword_lines() {
        let output = "line1\nAll tests passed ok\nline3\nERROR: something\nline5";
        let result = fallback_extract(output);
        assert!(result.contains("passed"));
        assert!(result.contains("ERROR"));
        assert!(!result.contains("line1"));
        assert!(!result.contains("line5"));
    }

    #[test]
    fn fallback_last_10_when_no_keywords() {
        let output = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl";
        let result = fallback_extract(output);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 10);
        assert_eq!(*lines.last().unwrap(), "l");
    }

    // -- generic framework filter outputs --

    #[test]
    fn mocha_filter_passing() {
        let output = "  suite\n    ok test one\n    ok test two\n\n  2 passing (34ms)";
        let result = filter_mocha(output, 0);
        assert!(result.contains("2 passing"));
    }

    #[test]
    fn playwright_filter_summary() {
        let output = "Running 3 tests\n\n  3 passed (1.5s)\n  0 failed\n  0 skipped";
        let result = filter_playwright(output, 0);
        assert!(result.contains("3 passed"));
        assert!(result.contains("0 failed"));
    }

    #[test]
    fn rspec_filter_summary() {
        let output = "....\n\nFinished in 0.5 seconds\n4 examples, 0 failures";
        let result = filter_rspec(output, 0);
        assert!(result.contains("4 examples, 0 failures"));
    }

    #[test]
    fn phpunit_filter_ok() {
        let output = "PHPUnit 10.0\n.....\n\nOK (5 tests, 10 assertions)";
        let result = filter_phpunit(output, 0);
        assert!(result.contains("OK (5 tests, 10 assertions)"));
    }

    #[test]
    fn dotnet_filter_passed() {
        let output = "Passed! - Failed: 0, Passed: 5\nTotal tests: 5";
        let result = filter_dotnet_test(output, 0);
        assert!(result.contains("Passed!"));
        assert!(result.contains("Total tests: 5"));
    }
}
