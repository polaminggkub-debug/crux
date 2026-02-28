use std::process::Command;

fn crux_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_crux"))
}

#[test]
fn run_echo_passthrough() {
    let output = crux_bin()
        .args(["run", "echo", "hello world"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello world"),
        "Expected passthrough, got: {stdout}"
    );
}

#[test]
fn run_false_reports_exit_code() {
    let output = crux_bin()
        .args(["run", "false"])
        .output()
        .expect("failed to execute crux");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("exit code"),
        "Expected exit code report, got: {stderr}"
    );
}

#[test]
fn which_git_status_resolves() {
    let output = crux_bin()
        .args(["which", "git", "status"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("git status"),
        "Expected git status filter match, got: {stdout}"
    );
}

#[test]
fn which_unknown_reports_no_match() {
    let output = crux_bin()
        .args(["which", "nonexistent-command-xyz"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No filter matches"),
        "Expected no match, got: {stdout}"
    );
}

#[test]
fn ls_lists_filters() {
    let output = crux_bin()
        .args(["ls"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("git status"),
        "Expected git status in list, got: {stdout}"
    );
    assert!(
        stdout.contains("cargo test"),
        "Expected cargo test in list, got: {stdout}"
    );
    assert!(
        stdout.contains("docker ps"),
        "Expected docker ps in list, got: {stdout}"
    );
}

#[test]
fn show_git_status_prints_details() {
    let output = crux_bin()
        .args(["show", "git status"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Command:") || stdout.contains("git status"),
        "Expected filter details, got: {stdout}"
    );
}

#[test]
fn eject_git_status_outputs_toml() {
    let output = crux_bin()
        .args(["eject", "git status"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("command = \"git status\""),
        "Expected TOML output, got: {stdout}"
    );
}

#[test]
fn err_filters_error_lines() {
    let output = crux_bin()
        .args([
            "err",
            "sh",
            "-c",
            "echo ok; echo 'error: bad thing'; echo done",
        ])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("error: bad thing"),
        "Expected error line kept, got: {stdout}"
    );
    assert!(
        !stdout.contains("ok"),
        "Expected non-error lines filtered out, got: {stdout}"
    );
}

#[test]
fn log_deduplicates_output() {
    let output = crux_bin()
        .args([
            "log",
            "sh",
            "-c",
            "echo line1; echo line1; echo line1; echo line2",
        ])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line1_count = stdout.matches("line1").count();
    assert_eq!(
        line1_count, 1,
        "Expected dedup to collapse line1, got {line1_count} occurrences"
    );
    assert!(stdout.contains("line2"));
}

#[test]
fn run_with_builtin_git_status_compresses() {
    // This test requires git to be available
    let output = crux_bin()
        .args(["run", "git", "status"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain branch info but not hints
    assert!(
        stdout.contains("On branch") || stdout.contains("nothing to commit"),
        "Expected compressed git status, got: {stdout}"
    );
    assert!(
        !stdout.contains("(use \"git restore"),
        "Hint lines should be stripped, got: {stdout}"
    );
}

#[test]
fn version_flag_works() {
    let output = crux_bin()
        .args(["--version"])
        .output()
        .expect("failed to execute crux");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("crux"),
        "Expected version output, got: {stdout}"
    );
}
