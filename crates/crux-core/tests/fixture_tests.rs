//! Fixture-based integration tests for crux-core builtin filters.
//!
//! Each test loads a realistic command output from `tests/fixtures/`,
//! runs it through the appropriate builtin filter, and asserts:
//!   - Compression occurred (output shorter than input)
//!   - Key content is preserved
//!   - Noise is removed

use crux_core::filter::builtin::registry;

// Embed fixtures at compile time so tests are self-contained.
const FIXTURE_GIT_STATUS_DIRTY: &str = include_str!("../../../tests/fixtures/git_status_dirty.txt");
const FIXTURE_GIT_STATUS_CLEAN: &str = include_str!("../../../tests/fixtures/git_status_clean.txt");
const FIXTURE_GIT_DIFF: &str = include_str!("../../../tests/fixtures/git_diff.txt");
const FIXTURE_GIT_LOG: &str = include_str!("../../../tests/fixtures/git_log.txt");
const FIXTURE_CARGO_TEST_PASS: &str = include_str!("../../../tests/fixtures/cargo_test_pass.txt");
const FIXTURE_CARGO_TEST_FAIL: &str = include_str!("../../../tests/fixtures/cargo_test_fail.txt");
const FIXTURE_CARGO_BUILD_ERRORS: &str =
    include_str!("../../../tests/fixtures/cargo_build_errors.txt");
const FIXTURE_DOCKER_PS: &str = include_str!("../../../tests/fixtures/docker_ps.txt");
const FIXTURE_GH_PR_LIST: &str = include_str!("../../../tests/fixtures/gh_pr_list.txt");
const FIXTURE_NPM_INSTALL: &str = include_str!("../../../tests/fixtures/npm_install.txt");

/// Helper: look up a builtin filter by command name and apply it.
fn apply_builtin(command: &str, output: &str, exit_code: i32) -> String {
    let reg = registry();
    let filter_fn = reg
        .get(command)
        .unwrap_or_else(|| panic!("No builtin filter registered for '{command}'"));
    filter_fn(output, exit_code)
}

// ---------------------------------------------------------------------------
// git status (dirty)
// ---------------------------------------------------------------------------

#[test]
fn git_status_dirty_compresses() {
    let result = apply_builtin("git status", FIXTURE_GIT_STATUS_DIRTY, 0);
    assert!(
        result.len() < FIXTURE_GIT_STATUS_DIRTY.len(),
        "Filtered output ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_GIT_STATUS_DIRTY.len()
    );
}

#[test]
fn git_status_dirty_preserves_branch_name() {
    let result = apply_builtin("git status", FIXTURE_GIT_STATUS_DIRTY, 0);
    assert!(
        result.contains("On branch feature/add-oauth-provider"),
        "Branch name must be preserved. Got:\n{result}"
    );
}

#[test]
fn git_status_dirty_preserves_file_statuses() {
    let result = apply_builtin("git status", FIXTURE_GIT_STATUS_DIRTY, 0);
    // Staged files
    assert!(
        result.contains("M  src/auth/oauth.rs"),
        "Staged modified file missing"
    );
    assert!(
        result.contains("A  src/auth/providers/github.rs"),
        "Staged added file missing"
    );
    // Unstaged files
    assert!(
        result.contains("M  src/config.rs"),
        "Unstaged modified file missing"
    );
    // Untracked files
    assert!(result.contains("?? .env.local"), "Untracked file missing");
    assert!(
        result.contains("?? src/auth/providers/google.rs"),
        "Untracked file missing"
    );
}

#[test]
fn git_status_dirty_removes_hints() {
    let result = apply_builtin("git status", FIXTURE_GIT_STATUS_DIRTY, 0);
    assert!(
        !result.contains("use \"git restore"),
        "Hint text should be removed"
    );
    assert!(
        !result.contains("use \"git add"),
        "Hint text should be removed"
    );
    assert!(
        !result.contains("use \"git push"),
        "Hint text should be removed"
    );
}

#[test]
fn git_status_dirty_removes_section_headers() {
    let result = apply_builtin("git status", FIXTURE_GIT_STATUS_DIRTY, 0);
    assert!(
        !result.contains("Changes to be committed:"),
        "Section header should be removed"
    );
    assert!(
        !result.contains("Changes not staged for commit:"),
        "Section header should be removed"
    );
    assert!(
        !result.contains("Untracked files:"),
        "Section header should be removed"
    );
}

// ---------------------------------------------------------------------------
// git status (clean)
// ---------------------------------------------------------------------------

#[test]
fn git_status_clean_preserves_essentials() {
    let result = apply_builtin("git status", FIXTURE_GIT_STATUS_CLEAN, 0);
    assert!(result.contains("On branch main"));
    assert!(result.contains("nothing to commit"));
}

#[test]
fn git_status_clean_is_compact() {
    let result = apply_builtin("git status", FIXTURE_GIT_STATUS_CLEAN, 0);
    let line_count = result.lines().count();
    assert!(
        line_count <= 3,
        "Clean status should be at most 3 lines, got {line_count}"
    );
}

// ---------------------------------------------------------------------------
// git diff
// ---------------------------------------------------------------------------

#[test]
fn git_diff_compresses() {
    let result = apply_builtin("git diff", FIXTURE_GIT_DIFF, 0);
    assert!(
        result.len() < FIXTURE_GIT_DIFF.len(),
        "Filtered diff ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_GIT_DIFF.len()
    );
}

#[test]
fn git_diff_preserves_file_headers() {
    let result = apply_builtin("git diff", FIXTURE_GIT_DIFF, 0);
    assert!(
        result.contains("diff --git a/src/config.rs b/src/config.rs"),
        "File header for config.rs must be preserved"
    );
    assert!(
        result.contains("diff --git a/src/server.rs b/src/server.rs"),
        "File header for server.rs must be preserved"
    );
    assert!(
        result.contains("diff --git a/tests/server_test.rs b/tests/server_test.rs"),
        "File header for server_test.rs must be preserved"
    );
}

#[test]
fn git_diff_preserves_stat_summary() {
    let result = apply_builtin("git diff", FIXTURE_GIT_DIFF, 0);
    assert!(
        result.contains("3 files changed"),
        "Stat summary line must be preserved"
    );
}

#[test]
fn git_diff_summarizes_hunks_with_counts() {
    let result = apply_builtin("git diff", FIXTURE_GIT_DIFF, 0);
    // The diff has + and - lines in hunks; the filter should collapse them to (+N -M lines)
    assert!(
        result.contains("(+") && result.contains("lines)"),
        "Hunks should be summarized with add/delete counts. Got:\n{result}"
    );
}

#[test]
fn git_diff_removes_context_lines() {
    let result = apply_builtin("git diff", FIXTURE_GIT_DIFF, 0);
    // Context lines from the diff (unchanged code) should not appear verbatim
    assert!(
        !result.contains("use std::path::PathBuf"),
        "Unchanged context lines should be removed"
    );
    assert!(
        !result.contains("let listener = TcpListener::bind"),
        "Unchanged context lines should be removed"
    );
}

// ---------------------------------------------------------------------------
// git log
// ---------------------------------------------------------------------------

#[test]
fn git_log_compresses() {
    let result = apply_builtin("git log", FIXTURE_GIT_LOG, 0);
    assert!(
        result.len() < FIXTURE_GIT_LOG.len(),
        "Filtered log ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_GIT_LOG.len()
    );
}

#[test]
fn git_log_produces_one_line_per_commit() {
    let result = apply_builtin("git log", FIXTURE_GIT_LOG, 0);
    let line_count = result.lines().count();
    // The fixture has 10 commits
    assert_eq!(
        line_count, 10,
        "Should produce exactly 10 one-line entries, got {line_count}.\nOutput:\n{result}"
    );
}

#[test]
fn git_log_preserves_commit_hashes() {
    let result = apply_builtin("git log", FIXTURE_GIT_LOG, 0);
    assert!(
        result.contains("a1b2c3d"),
        "First commit hash must be present"
    );
    assert!(
        result.contains("d0e1f2a"),
        "Last commit hash must be present"
    );
}

#[test]
fn git_log_preserves_commit_messages() {
    let result = apply_builtin("git log", FIXTURE_GIT_LOG, 0);
    assert!(
        result.contains("add OAuth2 provider support"),
        "Commit message must be preserved"
    );
    assert!(
        result.contains("optimize N+1 queries"),
        "Commit message must be preserved"
    );
    assert!(
        result.contains("add cargo-deny audit"),
        "Last commit message must be preserved"
    );
}

#[test]
fn git_log_preserves_author_names() {
    let result = apply_builtin("git log", FIXTURE_GIT_LOG, 0);
    assert!(
        result.contains("Sarah Chen"),
        "Author name must be preserved"
    );
    assert!(
        result.contains("James Wilson"),
        "Author name must be preserved"
    );
}

#[test]
fn git_log_removes_date_lines() {
    let result = apply_builtin("git log", FIXTURE_GIT_LOG, 0);
    assert!(
        !result.contains("Date:"),
        "Date: lines should be removed from compact format"
    );
}

#[test]
fn git_log_removes_email_addresses() {
    let result = apply_builtin("git log", FIXTURE_GIT_LOG, 0);
    assert!(
        !result.contains("sarah.chen@example.com"),
        "Email addresses should be stripped"
    );
    assert!(
        !result.contains("jwilson@example.com"),
        "Email addresses should be stripped"
    );
}

// ---------------------------------------------------------------------------
// cargo test (passing)
// ---------------------------------------------------------------------------

#[test]
fn cargo_test_pass_compresses() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_PASS, 0);
    assert!(
        result.len() < FIXTURE_CARGO_TEST_PASS.len(),
        "Filtered output ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_CARGO_TEST_PASS.len()
    );
}

#[test]
fn cargo_test_pass_preserves_result_summary() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_PASS, 0);
    assert!(
        result.contains("test result: ok"),
        "Test result summary must be preserved. Got:\n{result}"
    );
}

#[test]
fn cargo_test_pass_removes_compiling_lines() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_PASS, 0);
    assert!(
        !result.contains("Compiling"),
        "'Compiling' lines should be removed"
    );
    assert!(
        !result.contains("Finished"),
        "'Finished' lines should be removed"
    );
}

#[test]
fn cargo_test_pass_removes_running_headers() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_PASS, 0);
    assert!(
        !result.contains("Running unittests"),
        "'Running unittests' headers should be removed"
    );
    assert!(
        !result.contains("Doc-tests"),
        "Doc-tests header should be removed"
    );
}

// ---------------------------------------------------------------------------
// cargo test (failing)
// ---------------------------------------------------------------------------

#[test]
fn cargo_test_fail_compresses() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_FAIL, 101);
    assert!(
        result.len() < FIXTURE_CARGO_TEST_FAIL.len(),
        "Filtered output ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_CARGO_TEST_FAIL.len()
    );
}

#[test]
fn cargo_test_fail_preserves_failure_summary() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_FAIL, 101);
    assert!(
        result.contains("FAILED"),
        "FAILED result must be preserved. Got:\n{result}"
    );
}

#[test]
fn cargo_test_fail_preserves_panic_info() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_FAIL, 101);
    assert!(
        result.contains("panicked at") || result.contains("assertion"),
        "Panic/assertion info must be preserved. Got:\n{result}"
    );
}

#[test]
fn cargo_test_fail_removes_compiling_lines() {
    let result = apply_builtin("cargo test", FIXTURE_CARGO_TEST_FAIL, 101);
    assert!(
        !result.contains("Compiling"),
        "'Compiling' lines should be removed"
    );
}

// ---------------------------------------------------------------------------
// cargo build (errors)
// ---------------------------------------------------------------------------

#[test]
fn cargo_build_errors_compresses() {
    let result = apply_builtin("cargo build", FIXTURE_CARGO_BUILD_ERRORS, 101);
    assert!(
        result.len() < FIXTURE_CARGO_BUILD_ERRORS.len(),
        "Filtered output ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_CARGO_BUILD_ERRORS.len()
    );
}

#[test]
fn cargo_build_errors_preserves_error_codes() {
    let result = apply_builtin("cargo build", FIXTURE_CARGO_BUILD_ERRORS, 101);
    assert!(
        result.contains("error[E0308]"),
        "Error code E0308 must be preserved"
    );
    assert!(
        result.contains("error[E0433]"),
        "Error code E0433 must be preserved"
    );
}

#[test]
fn cargo_build_errors_preserves_file_locations() {
    let result = apply_builtin("cargo build", FIXTURE_CARGO_BUILD_ERRORS, 101);
    assert!(
        result.contains("--> crates/crux-core/src/filter/builtin/cargo.rs:45:20"),
        "Error location must be preserved"
    );
    assert!(
        result.contains("--> crates/crux-core/src/filter/mod.rs:28:17"),
        "Error location must be preserved"
    );
}

#[test]
fn cargo_build_errors_removes_help_suggestions() {
    let result = apply_builtin("cargo build", FIXTURE_CARGO_BUILD_ERRORS, 101);
    assert!(
        !result.contains("help: try using a conversion method"),
        "Help suggestions should be removed"
    );
    assert!(
        !result.contains("help: consider importing"),
        "Help suggestions should be removed"
    );
}

#[test]
fn cargo_build_errors_removes_compiling_lines() {
    let result = apply_builtin("cargo build", FIXTURE_CARGO_BUILD_ERRORS, 101);
    assert!(
        !result.contains("Compiling crux-core"),
        "'Compiling' lines should be removed"
    );
}

// ---------------------------------------------------------------------------
// docker ps
// ---------------------------------------------------------------------------

#[test]
fn docker_ps_compresses() {
    let result = apply_builtin("docker ps", FIXTURE_DOCKER_PS, 0);
    assert!(
        result.len() < FIXTURE_DOCKER_PS.len(),
        "Filtered output ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_DOCKER_PS.len()
    );
}

#[test]
fn docker_ps_preserves_container_names() {
    let result = apply_builtin("docker ps", FIXTURE_DOCKER_PS, 0);
    assert!(
        result.contains("myapp-nginx-1"),
        "Container name must be preserved"
    );
    assert!(
        result.contains("myapp-api-1"),
        "Container name must be preserved"
    );
    assert!(
        result.contains("myapp-db-1"),
        "Container name must be preserved"
    );
    assert!(
        result.contains("myapp-redis-1"),
        "Container name must be preserved"
    );
    assert!(
        result.contains("monitoring-grafana-1"),
        "Container name must be preserved"
    );
}

#[test]
fn docker_ps_preserves_images() {
    let result = apply_builtin("docker ps", FIXTURE_DOCKER_PS, 0);
    assert!(
        result.contains("nginx:1.25-alpine"),
        "Image name must be preserved"
    );
    assert!(
        result.contains("postgres:16.2-alpine"),
        "Image name must be preserved"
    );
}

#[test]
fn docker_ps_preserves_status() {
    let result = apply_builtin("docker ps", FIXTURE_DOCKER_PS, 0);
    assert!(
        result.contains("Up 2 hours"),
        "Container status must be preserved"
    );
}

#[test]
fn docker_ps_strips_ports_column() {
    let result = apply_builtin("docker ps", FIXTURE_DOCKER_PS, 0);
    assert!(
        !result.contains("0.0.0.0:80->80/tcp"),
        "PORTS column data should be stripped"
    );
    assert!(
        !result.contains("0.0.0.0:5432->5432/tcp"),
        "PORTS column data should be stripped"
    );
}

// ---------------------------------------------------------------------------
// gh pr list
// ---------------------------------------------------------------------------

#[test]
fn gh_pr_list_preserves_pr_numbers() {
    let result = apply_builtin("gh pr list", FIXTURE_GH_PR_LIST, 0);
    assert!(result.contains("#342"), "PR number must be preserved");
    assert!(result.contains("#339"), "PR number must be preserved");
}

#[test]
fn gh_pr_list_preserves_pr_titles() {
    let result = apply_builtin("gh pr list", FIXTURE_GH_PR_LIST, 0);
    assert!(
        result.contains("add OAuth2 provider support"),
        "PR title must be preserved"
    );
}

#[test]
fn gh_pr_list_removes_showing_footer() {
    let result = apply_builtin("gh pr list", FIXTURE_GH_PR_LIST, 0);
    assert!(
        !result.contains("Showing 8 of 42"),
        "'Showing X of Y' footer should be removed"
    );
}

// ---------------------------------------------------------------------------
// npm install
// ---------------------------------------------------------------------------

#[test]
fn npm_install_compresses() {
    let result = apply_builtin("npm install", FIXTURE_NPM_INSTALL, 0);
    assert!(
        result.len() < FIXTURE_NPM_INSTALL.len(),
        "Filtered output ({} bytes) should be shorter than input ({} bytes)",
        result.len(),
        FIXTURE_NPM_INSTALL.len()
    );
}

#[test]
fn npm_install_preserves_package_count() {
    let result = apply_builtin("npm install", FIXTURE_NPM_INSTALL, 0);
    assert!(
        result.contains("847 packages") || result.contains("added 847"),
        "Package count must be preserved. Got:\n{result}"
    );
}

#[test]
fn npm_install_preserves_added_packages_line() {
    let result = apply_builtin("npm install", FIXTURE_NPM_INSTALL, 0);
    assert!(
        result.contains("added 847 packages"),
        "Package added summary must be preserved. Got:\n{result}"
    );
}

#[test]
fn npm_install_removes_funding_and_audit_hints() {
    let result = apply_builtin("npm install", FIXTURE_NPM_INSTALL, 0);
    assert!(
        !result.contains("run `npm fund`"),
        "Funding hint should be removed"
    );
    assert!(
        !result.contains("npm audit fix --force"),
        "Audit fix suggestion should be removed"
    );
}

// ---------------------------------------------------------------------------
// Cross-cutting: all fixtures compress
// ---------------------------------------------------------------------------

#[test]
fn all_fixtures_produce_nonempty_output() {
    let cases: Vec<(&str, &str, i32)> = vec![
        ("git status", FIXTURE_GIT_STATUS_DIRTY, 0),
        ("git status", FIXTURE_GIT_STATUS_CLEAN, 0),
        ("git diff", FIXTURE_GIT_DIFF, 0),
        ("git log", FIXTURE_GIT_LOG, 0),
        ("cargo test", FIXTURE_CARGO_TEST_PASS, 0),
        ("cargo test", FIXTURE_CARGO_TEST_FAIL, 101),
        ("cargo build", FIXTURE_CARGO_BUILD_ERRORS, 101),
        ("docker ps", FIXTURE_DOCKER_PS, 0),
        ("gh pr list", FIXTURE_GH_PR_LIST, 0),
        ("npm install", FIXTURE_NPM_INSTALL, 0),
    ];

    for (command, fixture, exit_code) in cases {
        let result = apply_builtin(command, fixture, exit_code);
        assert!(
            !result.is_empty(),
            "Filter for '{command}' should produce non-empty output"
        );
    }
}
