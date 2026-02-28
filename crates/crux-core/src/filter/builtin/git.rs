use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register core git handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("git status", filter_git_status as BuiltinFilterFn);
    m.insert("git diff", filter_git_diff as BuiltinFilterFn);
    m.insert("git log", filter_git_log as BuiltinFilterFn);
    m.insert("git push", filter_git_push as BuiltinFilterFn);
}

/// Filter git status: keep branch line and file status lines, strip hints and boilerplate.
/// Normalizes long-format lines to short format and compresses branch tracking info.
pub fn filter_git_status(output: &str, _exit_code: i32) -> String {
    let mut lines = Vec::new();
    let mut in_untracked = false;
    let ahead_re = Regex::new(r"ahead of .+ by (\d+) commit").unwrap();
    let behind_re = Regex::new(r"behind .+ by (\d+) commit").unwrap();

    for line in output.lines() {
        let trimmed = line.trim();

        // Track when we enter/leave the "Untracked files:" section
        if trimmed == "Untracked files:" {
            in_untracked = true;
            continue;
        }
        // Any other section header ends the untracked section
        if !trimmed.is_empty()
            && !line.starts_with('\t')
            && !line.starts_with("  ")
            && trimmed.ends_with(':')
        {
            in_untracked = false;
        }

        // Keep "On branch ..." line
        if trimmed.starts_with("On branch ") {
            lines.push(trimmed.to_string());
            continue;
        }

        // Compress branch tracking lines
        if trimmed.starts_with("Your branch is up to date") {
            // Skip entirely — up-to-date is the default assumption
            continue;
        }
        if trimmed.starts_with("Your branch is ahead") {
            if let Some(caps) = ahead_re.captures(trimmed) {
                lines.push(format!("ahead {}", &caps[1]));
            }
            continue;
        }
        if trimmed.starts_with("Your branch is behind") {
            if let Some(caps) = behind_re.captures(trimmed) {
                lines.push(format!("behind {}", &caps[1]));
            }
            continue;
        }
        // Keep diverged lines as-is (rare)
        if trimmed.starts_with("Your branch and") {
            lines.push(trimmed.to_string());
            continue;
        }

        // Keep file status lines (M, A, D, ??, R, C, U, etc.)
        // Matches short-format lines like "M  src/lib.rs" or "?? file.txt"
        // Also matches long-format status lines like "modified:   file"
        if is_status_file_line(trimmed) {
            if let Some(normalized) = normalize_status_line(trimmed) {
                lines.push(normalized);
            } else {
                lines.push(trimmed.to_string());
            }
            continue;
        }

        // In untracked section, indented non-hint lines are filenames
        if in_untracked
            && !trimmed.is_empty()
            && (line.starts_with('\t') || line.starts_with("  "))
            && !trimmed.starts_with('(')
        {
            lines.push(format!("?? {trimmed}"));
            continue;
        }

        // Keep "nothing to commit" line; skip "no changes added" (redundant)
        if trimmed.starts_with("nothing to commit") {
            lines.push(trimmed.to_string());
            continue;
        }

        // Skip everything else (hints, headers, blank lines, boilerplate)
    }

    if lines.is_empty() {
        "nothing to commit, working tree clean".to_string()
    } else {
        lines.join("\n")
    }
}

/// Normalize long-format status lines to short format.
/// Returns `Some(short)` if the line is long-format, `None` if already short.
fn normalize_status_line(line: &str) -> Option<String> {
    let long_re =
        Regex::new(r"^(modified|new file|deleted|renamed|copied|typechange):\s+(.+)$").unwrap();
    if let Some(caps) = long_re.captures(line) {
        let code = match &caps[1] {
            "modified" => "M",
            "new file" => "A",
            "deleted" => "D",
            "renamed" => "R",
            "copied" => "C",
            "typechange" => "T",
            _ => return None,
        };
        Some(format!("{}  {}", code, &caps[2]))
    } else {
        None
    }
}

fn is_status_file_line(line: &str) -> bool {
    // Short format: XY filename (e.g. "M  src/lib.rs", "?? new.txt", "AM file.rs")
    let re = Regex::new(r"^[MADRCU?! ]{1,2}\s+\S").unwrap();
    if re.is_match(line) {
        return true;
    }

    // Long format: "modified:   file", "new file:   file", "deleted:   file", etc.
    let long_re =
        Regex::new(r"^(modified|new file|deleted|renamed|copied|typechange):\s+\S").unwrap();
    long_re.is_match(line)
}

/// Filter git diff: keep file headers, stats summary, collapse large hunks.
pub fn filter_git_diff(output: &str, _exit_code: i32) -> String {
    let mut lines = Vec::new();
    let mut in_hunk = false;
    let mut hunk_adds: usize = 0;
    let mut hunk_dels: usize = 0;
    let mut hunk_file = String::new();

    for line in output.lines() {
        // File header lines
        if line.starts_with("diff --git") {
            flush_hunk(
                &mut lines,
                &mut in_hunk,
                &mut hunk_adds,
                &mut hunk_dels,
                &hunk_file,
            );
            lines.push(line.to_string());
            hunk_file = line.split(" b/").nth(1).unwrap_or("unknown").to_string();
            continue;
        }

        if line.starts_with("--- ") || line.starts_with("+++ ") {
            lines.push(line.to_string());
            continue;
        }

        // Stat summary at the end (e.g. " 3 files changed, 10 insertions(+)")
        if line.contains("files changed")
            || line.contains("file changed")
            || line.contains("insertions(+)")
            || line.contains("deletions(-)")
        {
            flush_hunk(
                &mut lines,
                &mut in_hunk,
                &mut hunk_adds,
                &mut hunk_dels,
                &hunk_file,
            );
            lines.push(line.to_string());
            continue;
        }

        // Hunk header
        if line.starts_with("@@") {
            flush_hunk(
                &mut lines,
                &mut in_hunk,
                &mut hunk_adds,
                &mut hunk_dels,
                &hunk_file,
            );
            lines.push(line.to_string());
            in_hunk = true;
            continue;
        }

        // Inside a hunk: count changes instead of showing every line
        if in_hunk {
            if line.starts_with('+') {
                hunk_adds += 1;
            } else if line.starts_with('-') {
                hunk_dels += 1;
            }
            continue;
        }

        // index line, mode changes — skip for brevity
    }

    flush_hunk(
        &mut lines,
        &mut in_hunk,
        &mut hunk_adds,
        &mut hunk_dels,
        &hunk_file,
    );

    if lines.is_empty() {
        "No changes.".to_string()
    } else {
        lines.join("\n")
    }
}

fn flush_hunk(
    lines: &mut Vec<String>,
    in_hunk: &mut bool,
    adds: &mut usize,
    dels: &mut usize,
    _file: &str,
) {
    if *in_hunk && (*adds > 0 || *dels > 0) {
        lines.push(format!("  (+{adds} -{dels} lines)"));
    }
    *in_hunk = false;
    *adds = 0;
    *dels = 0;
}

/// Filter git log: compact to one-line-per-commit format.
pub fn filter_git_log(output: &str, _exit_code: i32) -> String {
    let commit_re = Regex::new(r"^commit\s+([a-f0-9]{7,})").unwrap();
    let author_re = Regex::new(r"^Author:\s+(.+)").unwrap();

    // If no line starts with "commit " + hex hash, output is already compact (e.g., --oneline)
    let has_full_format = output.lines().any(|l| {
        l.starts_with("commit ") && l.as_bytes().get(7).is_some_and(|b| b.is_ascii_hexdigit())
    });
    if !has_full_format {
        return output.trim_end().to_string();
    }

    let mut result = Vec::new();
    let mut current_hash = String::new();
    let mut current_author = String::new();
    let mut current_message = String::new();

    for line in output.lines() {
        if let Some(caps) = commit_re.captures(line) {
            // Flush previous commit
            if !current_hash.is_empty() {
                result.push(format_commit(
                    &current_hash,
                    &current_author,
                    &current_message,
                ));
            }
            current_hash = caps[1][..7.min(caps[1].len())].to_string();
            current_author.clear();
            current_message.clear();
            continue;
        }

        if let Some(caps) = author_re.captures(line) {
            current_author = caps[1].to_string();
            // Strip email if present
            if let Some(idx) = current_author.find('<') {
                current_author = current_author[..idx].trim().to_string();
            }
            continue;
        }

        // Skip Date: line
        if line.starts_with("Date:") || line.trim().is_empty() {
            continue;
        }

        // Commit message body — take first non-empty line
        if current_message.is_empty() {
            let msg = line.trim();
            if !msg.is_empty() {
                current_message = msg.to_string();
            }
        }
    }

    // Flush last commit
    if !current_hash.is_empty() {
        result.push(format_commit(
            &current_hash,
            &current_author,
            &current_message,
        ));
    }

    // If input was already one-line format, pass through
    if result.is_empty() && !output.trim().is_empty() {
        return output.to_string();
    }

    result.join("\n")
}

fn format_commit(hash: &str, author: &str, message: &str) -> String {
    if author.is_empty() {
        format!("{hash} {message}")
    } else {
        format!("{hash} ({author}) {message}")
    }
}

/// Filter git push: keep only the result line and any errors.
pub fn filter_git_push(output: &str, exit_code: i32) -> String {
    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Keep the branch push result line (e.g. "main -> main" or "abc123..def456  main -> main")
        if trimmed.contains("->") && !trimmed.starts_with("remote:") {
            lines.push(trimmed.to_string());
            continue;
        }

        // Keep error/fatal lines
        if trimmed.starts_with("error:")
            || trimmed.starts_with("fatal:")
            || trimmed.starts_with("!")
        {
            lines.push(trimmed.to_string());
            continue;
        }

        // Keep "Everything up-to-date"
        if trimmed == "Everything up-to-date" {
            lines.push(trimmed.to_string());
            continue;
        }

        // Keep remote rejection messages
        if trimmed.starts_with("remote: error") || trimmed.starts_with("remote: rejected") {
            lines.push(trimmed.to_string());
            continue;
        }
    }

    if lines.is_empty() {
        if exit_code != 0 {
            format!("Push failed (exit code {exit_code})")
        } else {
            "Push completed.".to_string()
        }
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- git status tests --

    #[test]
    fn git_status_filters_hints() {
        let input = r#"On branch main
Your branch is up to date with 'origin/main'.

Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
	M  src/lib.rs

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
	M  src/main.rs

Untracked files:
  (use "git add <file>..." to include in what will be committed)
	?? new_file.txt"#;

        let result = filter_git_status(input, 0);
        assert!(result.contains("On branch main"));
        assert!(result.contains("M  src/lib.rs"));
        assert!(result.contains("M  src/main.rs"));
        assert!(result.contains("?? new_file.txt"));
        assert!(!result.contains("use \"git"));
        assert!(!result.contains("Your branch"), "got: {result}");
    }

    #[test]
    fn git_status_long_format() {
        let input = r#"On branch main
Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
	modified:   test.txt

Untracked files:
  (use "git add <file>..." to include in what will be committed)
	new.txt

no changes added to commit (use "git add" and/or "git commit -a")"#;

        let result = filter_git_status(input, 0);
        assert!(result.contains("On branch main"), "missing branch line");
        assert!(
            result.contains("M  test.txt"),
            "missing modified file: got: {result}"
        );
        assert!(
            result.contains("?? new.txt"),
            "missing untracked file: got: {result}"
        );
        // "no changes added" is now skipped as redundant
        assert!(
            !result.contains("no changes added"),
            "should skip redundant line: got: {result}"
        );
        // Hint lines like '  (use "git restore..." ...)' should be stripped
        assert!(
            !result.contains("restore"),
            "should not contain hint lines: got: {result}"
        );
        assert!(
            !result.contains("Changes not staged"),
            "should not contain section headers: got: {result}"
        );
    }

    #[test]
    fn git_status_long_format_staged() {
        let input = r#"On branch feature
Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
	new file:   src/new.rs
	modified:   src/lib.rs

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
	deleted:    old.txt"#;

        let result = filter_git_status(input, 0);
        assert!(result.contains("A  src/new.rs"), "got: {result}");
        assert!(result.contains("M  src/lib.rs"), "got: {result}");
        assert!(result.contains("D  old.txt"), "got: {result}");
    }

    #[test]
    fn git_status_clean() {
        let input = r#"On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean"#;

        let result = filter_git_status(input, 0);
        assert!(result.contains("On branch main"));
        assert!(result.contains("nothing to commit"));
        assert!(!result.contains("Your branch"), "got: {result}");
        assert!(!result.contains("up to date"), "got: {result}");
    }

    #[test]
    fn git_status_normalizes_long_format() {
        let input = "On branch main\nChanges to be committed:\n\tnew file:   src/new.rs\n\tmodified:   src/lib.rs\n\nChanges not staged for commit:\n\tdeleted:    old.txt\n\trenamed:    a.rs -> b.rs";
        let result = filter_git_status(input, 0);
        assert!(result.contains("A  src/new.rs"), "got: {result}");
        assert!(result.contains("M  src/lib.rs"), "got: {result}");
        assert!(result.contains("D  old.txt"), "got: {result}");
        assert!(result.contains("R  a.rs -> b.rs"), "got: {result}");
    }

    #[test]
    fn git_status_compresses_ahead() {
        let input = "On branch main\nYour branch is ahead of 'origin/main' by 3 commits.\n  (use \"git push\" to publish your local commits)\n\nnothing to commit, working tree clean";
        let result = filter_git_status(input, 0);
        assert!(result.contains("ahead 3"), "got: {result}");
        assert!(!result.contains("Your branch"), "got: {result}");
    }

    #[test]
    fn git_status_compresses_behind() {
        let input = "On branch main\nYour branch is behind 'origin/main' by 5 commits, and can be fast-forwarded.\n  (use \"git pull\" to update your local branch)\n\nnothing to commit, working tree clean";
        let result = filter_git_status(input, 0);
        assert!(result.contains("behind 5"), "got: {result}");
        assert!(!result.contains("Your branch"), "got: {result}");
    }

    #[test]
    fn git_status_skips_up_to_date() {
        let input = "On branch main\nYour branch is up to date with 'origin/main'.\n\nnothing to commit, working tree clean";
        let result = filter_git_status(input, 0);
        assert!(!result.contains("Your branch"), "got: {result}");
        assert!(!result.contains("up to date"), "got: {result}");
    }

    // -- git diff tests --

    #[test]
    fn git_diff_summarizes_hunks() {
        let input = r#"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,7 @@
 use std::io;
+use std::fs;
+use std::path::Path;

 fn main() {
-    println!("old");
+    println!("new");
 }"#;

        let result = filter_git_diff(input, 0);
        assert!(result.contains("diff --git"));
        assert!(result.contains("--- a/src/lib.rs"));
        assert!(result.contains("+++ b/src/lib.rs"));
        assert!(result.contains("(+3 -1 lines)"));
    }

    #[test]
    fn git_diff_empty() {
        let result = filter_git_diff("", 0);
        assert_eq!(result, "No changes.");
    }

    // -- git log tests --

    #[test]
    fn git_log_compacts_commits() {
        let input = r#"commit abc1234def5678
Author: John Doe <john@example.com>
Date:   Mon Jan 1 00:00:00 2024 +0000

    Initial commit

commit def5678abc1234
Author: Jane Smith <jane@example.com>
Date:   Tue Jan 2 00:00:00 2024 +0000

    Add feature X"#;

        let result = filter_git_log(input, 0);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("abc1234"));
        assert!(lines[0].contains("John Doe"));
        assert!(lines[0].contains("Initial commit"));
        assert!(lines[1].contains("def5678"));
        assert!(lines[1].contains("Add feature X"));
    }

    #[test]
    fn git_log_oneline_passthrough() {
        let input = "abc1234 Initial commit\ndef5678 Add feature X\n1234567 Fix bug";
        let result = filter_git_log(input, 0);
        assert_eq!(result, input.trim_end());
    }

    #[test]
    fn git_log_short_format_passthrough() {
        let input = "abc1234 (HEAD -> main, origin/main) Initial commit\ndef5678 Add feature X";
        let result = filter_git_log(input, 0);
        assert_eq!(result, input);
    }

    // -- git push tests --

    #[test]
    fn git_push_keeps_result() {
        let input = r#"Enumerating objects: 5, done.
Counting objects: 100% (5/5), done.
Delta compression using up to 8 threads
Compressing objects: 100% (3/3), done.
Writing objects: 100% (3/3), 284 bytes | 284.00 KiB/s, done.
Total 3 (delta 2), reused 0 (delta 0), pack-reused 0
   abc1234..def5678  main -> main"#;

        let result = filter_git_push(input, 0);
        assert!(result.contains("main -> main"));
        assert!(!result.contains("Enumerating"));
        assert!(!result.contains("Compressing"));
    }

    #[test]
    fn git_push_up_to_date() {
        let input = "Everything up-to-date";
        let result = filter_git_push(input, 0);
        assert_eq!(result, "Everything up-to-date");
    }

    #[test]
    fn git_push_error() {
        let input = r#"error: failed to push some refs to 'origin'
! [rejected]        main -> main (non-fast-forward)"#;
        let result = filter_git_push(input, 1);
        assert!(result.contains("error: failed to push"));
        assert!(result.contains("[rejected]"));
    }
}
