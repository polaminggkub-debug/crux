use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register extended git command handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("git show", filter_git_show as BuiltinFilterFn);
    m.insert("git branch", filter_git_branch as BuiltinFilterFn);
    m.insert("git commit", filter_git_commit as BuiltinFilterFn);
    m.insert("git add", filter_git_add as BuiltinFilterFn);
    m.insert("git fetch", filter_git_fetch as BuiltinFilterFn);
    m.insert("git pull", filter_git_pull as BuiltinFilterFn);
    m.insert("git stash", filter_git_stash as BuiltinFilterFn);
}

/// Filter git show: keep commit metadata and diffstat, summarize diff body.
fn filter_git_show(output: &str, _exit_code: i32) -> String {
    let mut lines = Vec::new();
    let mut in_diff = false;
    let mut diff_adds: usize = 0;
    let mut diff_dels: usize = 0;
    let stat_re = Regex::new(r"^\s*\d+ files? changed").unwrap();

    for line in output.lines() {
        if line.starts_with("commit ") && !in_diff {
            lines.push(line.to_string());
            continue;
        }
        if line.starts_with("Author:") || line.starts_with("Date:") {
            lines.push(line.to_string());
            continue;
        }
        // Commit message lines (indented with spaces, before diff)
        if !in_diff && line.starts_with("    ") {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
            continue;
        }
        // Diffstat summary line
        if stat_re.is_match(line) {
            lines.push(line.trim().to_string());
            continue;
        }
        // Diff starts
        if line.starts_with("diff --git") {
            in_diff = true;
            continue;
        }
        if in_diff {
            if line.starts_with('+') && !line.starts_with("+++") {
                diff_adds += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                diff_dels += 1;
            }
        }
    }

    if in_diff && (diff_adds > 0 || diff_dels > 0) {
        lines.push(format!("Diff: +{diff_adds} -{diff_dels} lines"));
    }

    if lines.is_empty() {
        output.to_string()
    } else {
        lines.join("\n")
    }
}

/// Filter git branch: keep branch names, strip remote tracking noise.
fn filter_git_branch(output: &str, _exit_code: i32) -> String {
    let head_re = Regex::new(r"remotes/origin/HEAD\s*->").unwrap();
    let tracking_re = Regex::new(r"\s*\[.*\]").unwrap();

    let mut lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip remotes/origin/HEAD -> ... lines
        if head_re.is_match(trimmed) {
            continue;
        }
        // Strip tracking info in brackets like [ahead 1, behind 2]
        let cleaned = tracking_re.replace(trimmed, "").to_string();
        let cleaned = cleaned.trim();
        if !cleaned.is_empty() {
            lines.push(cleaned.to_string());
        }
    }

    if lines.is_empty() {
        "No branches.".to_string()
    } else {
        lines.join("\n")
    }
}

/// Filter git commit: keep summary line and file change stats, drop diff.
fn filter_git_commit(output: &str, exit_code: i32) -> String {
    let summary_re = Regex::new(r"^\[.+\s+[a-f0-9]+\]").unwrap();
    let stat_re = Regex::new(r"^\s*\d+ files? changed").unwrap();
    let mode_re = Regex::new(r"^\s*(create|delete|rename) mode").unwrap();

    let mut lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if summary_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }
        if stat_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }
        if mode_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep error/abort messages
        if trimmed.starts_with("error:") || trimmed.starts_with("fatal:") {
            lines.push(trimmed.to_string());
            continue;
        }
    }

    if lines.is_empty() {
        if exit_code != 0 {
            format!("Commit failed (exit code {exit_code})")
        } else {
            "Committed.".to_string()
        }
    } else {
        lines.join("\n")
    }
}

/// Filter git add: on success return "Staged.", on error keep error lines.
fn filter_git_add(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        let has_error = output
            .lines()
            .any(|l| l.starts_with("fatal:") || l.starts_with("error:"));
        if has_error {
            return output
                .lines()
                .filter(|l| l.starts_with("fatal:") || l.starts_with("error:"))
                .collect::<Vec<_>>()
                .join("\n");
        }
        return "Staged.".to_string();
    }
    // Non-zero exit: keep error/fatal lines
    let errors: Vec<&str> = output
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("fatal:") || t.starts_with("error:") || t.starts_with("hint:")
        })
        .collect();
    if errors.is_empty() {
        format!("git add failed (exit code {exit_code})")
    } else {
        errors.join("\n")
    }
}

/// Filter git fetch: keep "From" and new ref lines, drop progress.
fn filter_git_fetch(output: &str, _exit_code: i32) -> String {
    let progress_re = Regex::new(r"(?i)(counting|compressing|receiving|resolving)\s").unwrap();

    let mut lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if progress_re.is_match(trimmed) {
            continue;
        }
        // Keep "From ..." lines
        if trimmed.starts_with("From ") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep new branch/tag lines like " * [new branch]" or " * [new tag]"
        if trimmed.contains("[new branch]")
            || trimmed.contains("[new tag]")
            || trimmed.contains("[new ref]")
        {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep update lines like "abc123..def456  main -> origin/main"
        if trimmed.contains("->") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep error lines
        if trimmed.starts_with("fatal:") || trimmed.starts_with("error:") {
            lines.push(trimmed.to_string());
            continue;
        }
    }

    if lines.is_empty() {
        "Already up to date.".to_string()
    } else {
        lines.join("\n")
    }
}

/// Filter git pull: keep merge result, file changes, conflicts. Drop progress.
fn filter_git_pull(output: &str, _exit_code: i32) -> String {
    let progress_re = Regex::new(r"(?i)(counting|compressing|receiving|resolving deltas)").unwrap();
    let stat_re = Regex::new(r"^\s*\d+ files? changed").unwrap();

    let mut lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if progress_re.is_match(trimmed) {
            continue;
        }
        // Keep "Already up to date." / "Already up-to-date."
        if trimmed.starts_with("Already up") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep merge strategy lines like "Updating abc..def" or "Fast-forward"
        if trimmed.starts_with("Updating ") || trimmed.starts_with("Fast-forward") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep file change summary
        if stat_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep per-file stat lines like " src/lib.rs | 5 ++-"
        if trimmed.contains(" | ") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep conflict lines
        if trimmed.starts_with("CONFLICT") || trimmed.starts_with("Merge conflict") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep error/fatal
        if trimmed.starts_with("error:") || trimmed.starts_with("fatal:") {
            lines.push(trimmed.to_string());
            continue;
        }
    }

    if lines.is_empty() {
        "Pull completed.".to_string()
    } else {
        lines.join("\n")
    }
}

/// Filter git stash: keep stash save confirmations and list entries, drop diffs.
fn filter_git_stash(output: &str, _exit_code: i32) -> String {
    let stash_entry_re = Regex::new(r"^stash@\{\d+\}:").unwrap();

    let mut lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Keep "Saved working directory..." line
        if trimmed.starts_with("Saved working directory") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep stash list entries
        if stash_entry_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep "Dropped" or "Applied" messages
        if trimmed.starts_with("Dropped") || trimmed.starts_with("Applied") {
            lines.push(trimmed.to_string());
            continue;
        }
        // Keep "No stash entries found."
        if trimmed.contains("No stash entries") || trimmed.contains("No stash found") {
            lines.push(trimmed.to_string());
            continue;
        }
    }

    if lines.is_empty() {
        output.trim().to_string()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- git show tests --

    #[test]
    fn git_show_keeps_metadata_and_summarizes_diff() {
        let input = concat!(
            "commit abc1234def5678\n",
            "Author: Alice <alice@example.com>\n",
            "Date:   Mon Jan 1 00:00:00 2024 +0000\n",
            "\n",
            "    Fix the bug\n",
            "\n",
            "diff --git a/src/lib.rs b/src/lib.rs\n",
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -1,3 +1,4 @@\n",
            "+use std::fs;\n",
            " fn main() {}\n",
            "-old line\n",
            "+new line\n",
        );
        let result = filter_git_show(input, 0);
        assert!(result.contains("commit abc1234def5678"));
        assert!(result.contains("Author:"));
        assert!(result.contains("Fix the bug"));
        assert!(result.contains("Diff: +2 -1 lines"));
        assert!(!result.contains("use std::fs"));
    }

    #[test]
    fn git_show_keeps_diffstat_summary() {
        let input = concat!(
            "commit abc1234\n",
            "Author: Bob <bob@x.com>\n",
            "Date:   Tue Jan 2 00:00:00 2024\n",
            "\n",
            "    Add feature\n",
            "\n",
            " 2 files changed, 10 insertions(+), 3 deletions(-)\n",
            "\n",
            "diff --git a/a.rs b/a.rs\n",
            "+added\n",
        );
        let result = filter_git_show(input, 0);
        assert!(result.contains("2 files changed"));
        assert!(result.contains("Diff: +1 -0 lines"));
    }

    #[test]
    fn git_show_no_diff() {
        let input = concat!(
            "commit abc1234\n",
            "Author: Eve <eve@x.com>\n",
            "Date:   Wed Jan 3 00:00:00 2024\n",
            "\n",
            "    Empty commit\n",
        );
        let result = filter_git_show(input, 0);
        assert!(result.contains("commit abc1234"));
        assert!(result.contains("Empty commit"));
        assert!(!result.contains("Diff:"));
    }

    // -- git branch tests --

    #[test]
    fn git_branch_keeps_names_with_current_marker() {
        let input = "  develop\n\
                      * main\n\
                        feature/x\n";
        let result = filter_git_branch(input, 0);
        assert!(result.contains("* main"));
        assert!(result.contains("develop"));
        assert!(result.contains("feature/x"));
    }

    #[test]
    fn git_branch_strips_remote_head() {
        let input = "  remotes/origin/HEAD -> origin/main\n\
                        remotes/origin/main\n\
                        remotes/origin/develop\n";
        let result = filter_git_branch(input, 0);
        assert!(!result.contains("HEAD ->"));
        assert!(result.contains("remotes/origin/main"));
        assert!(result.contains("remotes/origin/develop"));
    }

    #[test]
    fn git_branch_strips_tracking_info() {
        let input = "* main [ahead 2, behind 1]\n\
                        develop [behind 3]\n";
        let result = filter_git_branch(input, 0);
        assert!(result.contains("* main"));
        assert!(!result.contains("[ahead"));
        assert!(!result.contains("[behind"));
    }

    // -- git commit tests --

    #[test]
    fn git_commit_keeps_summary_and_stats() {
        let input = "[main abc1234] Fix bug in parser\n\
                       2 files changed, 10 insertions(+), 3 deletions(-)\n\
                       create mode 100644 src/new.rs\n";
        let result = filter_git_commit(input, 0);
        assert!(result.contains("[main abc1234] Fix bug in parser"));
        assert!(result.contains("2 files changed"));
        assert!(result.contains("create mode"));
    }

    #[test]
    fn git_commit_drops_verbose_diff() {
        let input = "[main abc1234] Add feature\n\
                       1 file changed, 5 insertions(+)\n\
                      diff --git a/src/lib.rs b/src/lib.rs\n\
                      +new line\n\
                      -old line\n";
        let result = filter_git_commit(input, 0);
        assert!(result.contains("[main abc1234]"));
        assert!(!result.contains("diff --git"));
        assert!(!result.contains("+new line"));
    }

    #[test]
    fn git_commit_error() {
        let input = "error: pathspec 'nonexistent' did not match any files\n";
        let result = filter_git_commit(input, 1);
        assert!(result.contains("error: pathspec"));
    }

    // -- git add tests --

    #[test]
    fn git_add_success_returns_staged() {
        let result = filter_git_add("", 0);
        assert_eq!(result, "Staged.");
    }

    #[test]
    fn git_add_with_warnings_returns_staged() {
        let input = "warning: LF will be replaced by CRLF in file.txt.\n";
        let result = filter_git_add(input, 0);
        assert_eq!(result, "Staged.");
    }

    #[test]
    fn git_add_error_keeps_message() {
        let input = "fatal: pathspec 'nope' did not match any files\n";
        let result = filter_git_add(input, 128);
        assert!(result.contains("fatal: pathspec"));
    }

    // -- git fetch tests --

    #[test]
    fn git_fetch_keeps_new_branches() {
        let input = "From github.com:user/repo\n\
                       * [new branch]      feature/x -> origin/feature/x\n\
                      Counting objects: 5, done.\n\
                      Compressing objects: 100%\n";
        let result = filter_git_fetch(input, 0);
        assert!(result.contains("From github.com:user/repo"));
        assert!(result.contains("[new branch]"));
        assert!(!result.contains("Counting"));
        assert!(!result.contains("Compressing"));
    }

    #[test]
    fn git_fetch_nothing_new() {
        let result = filter_git_fetch("", 0);
        assert_eq!(result, "Already up to date.");
    }

    #[test]
    fn git_fetch_keeps_update_refs() {
        let input = "From github.com:user/repo\n\
                       abc1234..def5678  main -> origin/main\n";
        let result = filter_git_fetch(input, 0);
        assert!(result.contains("main -> origin/main"));
    }

    // -- git pull tests --

    #[test]
    fn git_pull_keeps_merge_result() {
        let input = "Counting objects: 5, done.\n\
                      Compressing objects: 100%\n\
                      Updating abc1234..def5678\n\
                      Fast-forward\n\
                       src/lib.rs | 5 ++---\n\
                       1 file changed, 2 insertions(+), 3 deletions(-)\n";
        let result = filter_git_pull(input, 0);
        assert!(result.contains("Updating abc1234..def5678"));
        assert!(result.contains("Fast-forward"));
        assert!(result.contains("src/lib.rs | 5 ++---"));
        assert!(result.contains("1 file changed"));
        assert!(!result.contains("Counting"));
    }

    #[test]
    fn git_pull_already_up_to_date() {
        let input = "Already up to date.\n";
        let result = filter_git_pull(input, 0);
        assert!(result.contains("Already up to date."));
    }

    #[test]
    fn git_pull_keeps_conflicts() {
        let input = "Updating abc..def\n\
                      CONFLICT (content): Merge conflict in src/lib.rs\n\
                      error: could not apply abc1234\n";
        let result = filter_git_pull(input, 1);
        assert!(result.contains("CONFLICT"));
        assert!(result.contains("error:"));
    }

    // -- git stash tests --

    #[test]
    fn git_stash_keeps_save_message() {
        let input = "Saved working directory and index state WIP on main: abc1234 Fix bug\n";
        let result = filter_git_stash(input, 0);
        assert!(result.contains("Saved working directory"));
    }

    #[test]
    fn git_stash_keeps_list_entries() {
        let input = "stash@{0}: WIP on main: abc1234 Fix bug\n\
                      stash@{1}: On develop: wip feature\n";
        let result = filter_git_stash(input, 0);
        assert!(result.contains("stash@{0}:"));
        assert!(result.contains("stash@{1}:"));
    }

    #[test]
    fn git_stash_drops_diff_details() {
        let input = "Saved working directory and index state WIP on main: abc Fix\n\
                      diff --git a/file.rs b/file.rs\n\
                      +added line\n\
                      -removed line\n";
        let result = filter_git_stash(input, 0);
        assert!(result.contains("Saved working directory"));
        assert!(!result.contains("diff --git"));
        assert!(!result.contains("+added"));
    }
}
