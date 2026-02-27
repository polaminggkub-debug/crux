use std::collections::HashMap;

use super::BuiltinFilterFn;

/// Register filesystem command handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("ls", filter_ls as BuiltinFilterFn);
    m.insert("find", filter_find as BuiltinFilterFn);
    m.insert("grep", filter_grep as BuiltinFilterFn);
    m.insert("tree", filter_tree as BuiltinFilterFn);
    m.insert("cat", filter_cat as BuiltinFilterFn);
}

/// Strip ANSI escape sequences from text.
fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip until we hit a letter (end of ANSI sequence)
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Filter `ls`: if > 50 lines, show first 30 + truncation message.
pub fn filter_ls(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 50 {
        return output.to_string();
    }
    let remaining = lines.len() - 30;
    let mut result: Vec<&str> = lines[..30].to_vec();
    result.push("");
    let msg = format!("... and {remaining} more files");
    let mut out = result.join("\n");
    out.push('\n');
    out.push_str(&msg);
    out
}

/// Filter `find`: first 30 results + count. Remove "Permission denied" errors.
pub fn filter_find(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output
        .lines()
        .filter(|l| !l.contains("Permission denied"))
        .collect();

    if lines.len() <= 30 {
        return lines.join("\n");
    }
    let total = lines.len();
    let mut result: Vec<&str> = lines[..30].to_vec();
    result.push("");
    let msg = format!("... and {} more results ({total} total)", total - 30);
    let mut out = result.join("\n");
    out.push('\n');
    out.push_str(&msg);
    out
}

/// Filter `grep`: strip ANSI, truncate if > 50 matches, keep match count.
pub fn filter_grep(output: &str, _exit_code: i32) -> String {
    let cleaned = strip_ansi(output);
    let lines: Vec<&str> = cleaned.lines().collect();

    if lines.len() <= 50 {
        return cleaned;
    }
    let total = lines.len();
    let mut result: Vec<&str> = lines[..50].to_vec();
    result.push("");
    let msg = format!("... {total} total matches ({} more omitted)", total - 50);
    let mut out = result.join("\n");
    out.push('\n');
    out.push_str(&msg);
    out
}

/// Filter `tree`: if > 100 lines, truncate. Preserve summary line at end.
pub fn filter_tree(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 100 {
        return output.to_string();
    }

    // tree's last line is typically a summary like "N directories, M files"
    let last_line = lines.last().copied().unwrap_or("");
    let is_summary = last_line.contains("director") || last_line.contains("file");

    let shown = if is_summary { 99 } else { 100 };
    let omitted = lines.len() - shown - if is_summary { 1 } else { 0 };

    let mut result: Vec<&str> = lines[..shown].to_vec();
    result.push("");
    let msg = format!("... {omitted} more entries");
    let mut out = result.join("\n");
    out.push('\n');
    out.push_str(&msg);
    if is_summary {
        out.push('\n');
        out.push_str(last_line);
    }
    out
}

/// Filter `cat`: if > 200 lines, show first 50 + last 20 + summary.
pub fn filter_cat(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 200 {
        return output.to_string();
    }
    let total = lines.len();
    let head: Vec<&str> = lines[..50].to_vec();
    let tail: Vec<&str> = lines[total - 20..].to_vec();

    let mut out = head.join("\n");
    out.push_str("\n\n... (");
    out.push_str(&total.to_string());
    out.push_str(" lines total)\n\n");
    out.push_str(&tail.join("\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ls tests ----

    #[test]
    fn ls_passthrough_short() {
        let input = "file1.txt\nfile2.txt\nfile3.txt";
        let result = filter_ls(input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn ls_truncates_long() {
        let lines: Vec<String> = (0..80).map(|i| format!("file_{i}.txt")).collect();
        let input = lines.join("\n");
        let result = filter_ls(&input, 0);
        assert!(result.contains("file_0.txt"));
        assert!(result.contains("file_29.txt"));
        assert!(!result.contains("file_30.txt"));
        assert!(result.contains("... and 50 more files"));
    }

    #[test]
    fn ls_exactly_50_passthrough() {
        let lines: Vec<String> = (0..50).map(|i| format!("file_{i}.txt")).collect();
        let input = lines.join("\n");
        let result = filter_ls(&input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn ls_51_lines_truncates() {
        let lines: Vec<String> = (0..51).map(|i| format!("f{i}")).collect();
        let input = lines.join("\n");
        let result = filter_ls(&input, 0);
        assert!(result.contains("... and 21 more files"));
    }

    // ---- find tests ----

    #[test]
    fn find_removes_permission_denied() {
        let input = "/home/user/file.txt\nfind: '/root': Permission denied\n/home/user/other.txt";
        let result = filter_find(input, 1);
        assert!(result.contains("/home/user/file.txt"));
        assert!(result.contains("/home/user/other.txt"));
        assert!(!result.contains("Permission denied"));
    }

    #[test]
    fn find_truncates_long() {
        let lines: Vec<String> = (0..60).map(|i| format!("/path/file_{i}")).collect();
        let input = lines.join("\n");
        let result = filter_find(&input, 0);
        assert!(result.contains("/path/file_0"));
        assert!(result.contains("/path/file_29"));
        assert!(!result.contains("/path/file_30"));
        assert!(result.contains("... and 30 more results (60 total)"));
    }

    #[test]
    fn find_short_passthrough() {
        let input = "/a\n/b\n/c";
        let result = filter_find(input, 0);
        assert_eq!(result, input);
    }

    // ---- grep tests ----

    #[test]
    fn grep_strips_ansi_codes() {
        let input = "\x1b[35mfile.rs\x1b[0m:\x1b[32m10\x1b[0m:match line";
        let result = filter_grep(input, 0);
        assert!(!result.contains("\x1b["));
        assert!(result.contains("file.rs"));
        assert!(result.contains("match line"));
    }

    #[test]
    fn grep_truncates_long() {
        let lines: Vec<String> = (0..80).map(|i| format!("file.rs:{i}: matched")).collect();
        let input = lines.join("\n");
        let result = filter_grep(&input, 0);
        assert!(result.contains("file.rs:0: matched"));
        assert!(result.contains("file.rs:49: matched"));
        assert!(!result.contains("file.rs:50: matched"));
        assert!(result.contains("80 total matches"));
        assert!(result.contains("30 more omitted"));
    }

    #[test]
    fn grep_short_passthrough() {
        let input = "file.rs:1: hello\nfile.rs:5: world";
        let result = filter_grep(input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn grep_strips_ansi_and_truncates() {
        let lines: Vec<String> = (0..55)
            .map(|i| format!("\x1b[35mf.rs\x1b[0m:\x1b[32m{i}\x1b[0m: line"))
            .collect();
        let input = lines.join("\n");
        let result = filter_grep(&input, 0);
        assert!(!result.contains("\x1b["));
        assert!(result.contains("55 total matches"));
    }

    // ---- tree tests ----

    #[test]
    fn tree_short_passthrough() {
        let input = ".\n├── src\n│   └── main.rs\n└── Cargo.toml\n\n1 directory, 2 files";
        let result = filter_tree(input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn tree_truncates_with_summary() {
        let mut lines: Vec<String> = (0..120).map(|i| format!("├── file_{i}")).collect();
        lines.push("10 directories, 110 files".to_string());
        let input = lines.join("\n");
        let result = filter_tree(&input, 0);
        assert!(result.contains("├── file_0"));
        assert!(result.contains("├── file_98"));
        assert!(!result.contains("├── file_99"));
        assert!(result.contains("... 21 more entries"));
        assert!(result.contains("10 directories, 110 files"));
    }

    #[test]
    fn tree_truncates_without_summary() {
        let lines: Vec<String> = (0..110).map(|i| format!("├── item_{i}")).collect();
        let input = lines.join("\n");
        let result = filter_tree(&input, 0);
        assert!(result.contains("├── item_0"));
        assert!(result.contains("... 10 more entries"));
        assert!(!result.contains("├── item_100"));
    }

    // ---- cat tests ----

    #[test]
    fn cat_short_passthrough() {
        let input = "line 1\nline 2\nline 3";
        let result = filter_cat(input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn cat_truncates_long() {
        let lines: Vec<String> = (0..300).map(|i| format!("line {i}")).collect();
        let input = lines.join("\n");
        let result = filter_cat(&input, 0);
        assert!(result.contains("line 0"));
        assert!(result.contains("line 49"));
        assert!(result.contains("(300 lines total)"));
        assert!(result.contains("line 280"));
        assert!(result.contains("line 299"));
        // Middle lines should be omitted
        assert!(!result.contains("line 100"));
    }

    #[test]
    fn cat_exactly_200_passthrough() {
        let lines: Vec<String> = (0..200).map(|i| format!("line {i}")).collect();
        let input = lines.join("\n");
        let result = filter_cat(&input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn cat_201_truncates() {
        let lines: Vec<String> = (0..201).map(|i| format!("L{i}")).collect();
        let input = lines.join("\n");
        let result = filter_cat(&input, 0);
        assert!(result.contains("L0"));
        assert!(result.contains("L49"));
        assert!(result.contains("(201 lines total)"));
        assert!(result.contains("L181"));
        assert!(result.contains("L200"));
    }
}
