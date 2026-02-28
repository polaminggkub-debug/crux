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

/// Format a byte size into a human-readable string (e.g. 1647 -> "1.6K").
fn format_size(bytes: u64) -> String {
    if bytes < 1000 {
        return bytes.to_string();
    }
    let units = ["K", "M", "G", "T"];
    let mut size = bytes as f64;
    for unit in &units {
        size /= 1024.0;
        if size < 10.0 {
            return format!("{:.1}{unit}", size);
        }
        if size < 1000.0 {
            return format!("{:.0}{unit}", size);
        }
    }
    format!("{:.0}T", size)
}

/// Check if a line looks like `ls -l` long-format output (starts with permission bits).
fn is_ls_long_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.len() < 10 {
        return false;
    }
    let bytes = trimmed.as_bytes();
    // First char: d, -, l, c, b, p, s (file type indicators)
    matches!(bytes[0], b'd' | b'-' | b'l' | b'c' | b'b' | b'p' | b's')
        && bytes[1..10].iter().all(|&b| {
            matches!(
                b,
                b'r' | b'w' | b'x' | b'-' | b's' | b'S' | b't' | b'T' | b'@' | b'+'
            )
        })
}

/// Parse an `ls -l` line into (type_char, size, name).
/// Typical format: `drwxr-xr-x  12 polamin  staff  384 Feb  2 18:53 src`
/// Or with @:       `drwxr-xr-x@ 12 polamin  staff  384 Feb  2 18:53 src`
fn parse_ls_long_line(line: &str) -> Option<(char, u64, String)> {
    let trimmed = line.trim_start();
    if !is_ls_long_line(trimmed) {
        return None;
    }

    let type_char = trimmed.chars().next()?;

    // Split into whitespace-separated fields.
    // Fields: permissions, links, owner, group, size, month, day, time/year, name...
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    if fields.len() < 9 {
        return None;
    }

    // Size is field 4 (0-indexed)
    let size: u64 = fields[4].parse().ok()?;

    // Name is everything from field 8 onward (may contain spaces)
    let name = fields[8..].join(" ");

    // For symlinks, keep the full "name -> target"
    // For directories, append / if not already present
    let display_name = if type_char == 'd' && !name.ends_with('/') {
        format!("{name}/")
    } else {
        name
    };

    Some((type_char, size, display_name))
}

/// Filter `ls`: simplify long-format metadata, truncate if > 50 lines.
pub fn filter_ls(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();

    // Detect long-format output: check if at least one non-"total" line has permission bits
    let has_long_format = lines
        .iter()
        .any(|l| !l.trim_start().starts_with("total ") && is_ls_long_line(l));

    if has_long_format {
        return filter_ls_long(&lines);
    }

    // Simple ls output — just truncate if needed
    if lines.len() <= 50 {
        return output.to_string();
    }
    truncate_lines(&lines, 30, "files")
}

/// Simplify long-format ls output, then truncate if needed.
fn filter_ls_long(lines: &[&str]) -> String {
    let mut simplified: Vec<String> = Vec::with_capacity(lines.len());

    for line in lines {
        let trimmed = line.trim_start();
        // Skip "total X" lines
        if trimmed.starts_with("total ") {
            continue;
        }

        if let Some((type_char, size, name)) = parse_ls_long_line(line) {
            let size_str = format_size(size);
            simplified.push(format!("{type_char}  {size_str:>5}  {name}"));
        } else if !trimmed.is_empty() {
            // Keep unrecognized non-empty lines as-is
            simplified.push(trimmed.to_string());
        }
    }

    if simplified.len() > 50 {
        let remaining = simplified.len() - 30;
        let mut result: Vec<&str> = simplified[..30].iter().map(|s| s.as_str()).collect();
        result.push("");
        let msg = format!("... and {remaining} more files");
        let mut out = result.join("\n");
        out.push('\n');
        out.push_str(&msg);
        out
    } else {
        simplified.join("\n")
    }
}

/// Truncate lines with a summary message.
fn truncate_lines(lines: &[&str], keep: usize, noun: &str) -> String {
    let remaining = lines.len() - keep;
    let mut result: Vec<&str> = lines[..keep].to_vec();
    result.push("");
    let msg = format!("... and {remaining} more {noun}");
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

    // ---- format_size tests ----

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0");
        assert_eq!(format_size(384), "384");
        assert_eq!(format_size(999), "999");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0K");
        assert_eq!(format_size(1647), "1.6K");
        assert_eq!(format_size(10240), "10K");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1_048_576), "1.0M");
        assert_eq!(format_size(5_500_000), "5.2M");
    }

    // ---- ls tests (simple output) ----

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

    // ---- ls tests (long-format output) ----

    #[test]
    fn ls_long_small_directory_strips_metadata() {
        let input = "\
total 96
drwxr-xr-x@ 12 polamin  staff    384 Feb  2 18:53 src
-rw-r--r--   1 polamin  staff   1647 Jan 15 10:20 package.json
-rw-r--r--   1 polamin  staff  45678 Feb  1 09:00 Cargo.lock
lrwxr-xr-x   1 polamin  staff     20 Jan 10 08:00 link -> target";
        let result = filter_ls(input, 0);

        // "total" line should be stripped
        assert!(!result.contains("total 96"));

        // Owner, group, date should be stripped
        assert!(!result.contains("polamin"));
        assert!(!result.contains("staff"));
        assert!(!result.contains("Feb  2"));

        // Type indicator and name should be present
        assert!(result.contains("d"));
        assert!(result.contains("src/"));
        assert!(result.contains("package.json"));
        assert!(result.contains("Cargo.lock"));

        // Symlinks preserved
        assert!(result.contains("link -> target"));

        // Directories get / suffix
        assert!(result.contains("src/"));

        // Sizes are human-readable
        assert!(result.contains("1.6K"));
        assert!(result.contains("45K"));
    }

    #[test]
    fn ls_long_format_exact_output() {
        let input = "\
total 16
drwxr-xr-x  5 user  group   160 Feb  1 10:00 mydir
-rw-r--r--  1 user  group  2048 Feb  1 10:00 readme.md";
        let result = filter_ls(input, 0);

        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "d    160  mydir/");
        assert_eq!(lines[1], "-   2.0K  readme.md");
    }

    #[test]
    fn ls_long_large_directory_truncates() {
        // Generate 60 long-format lines
        let mut lines = vec!["total 1024".to_string()];
        for i in 0..60 {
            lines.push(format!(
                "-rw-r--r--  1 user  group  {size} Jan  1 00:00 file_{i:03}.txt",
                size = 100 + i
            ));
        }
        let input = lines.join("\n");
        let result = filter_ls(&input, 0);

        // Should be simplified
        assert!(!result.contains("user"));
        assert!(!result.contains("group"));
        assert!(!result.contains("total 1024"));

        // Should be truncated (60 entries > 50)
        assert!(result.contains("file_000.txt"));
        assert!(result.contains("file_029.txt"));
        assert!(!result.contains("file_030.txt"));
        assert!(result.contains("... and 30 more files"));
    }

    #[test]
    fn ls_long_with_extended_attributes() {
        let input = "\
total 8
drwxr-xr-x@ 3 user  staff  96 Feb  1 10:00 dir_with_xattr
-rw-r--r--@ 1 user  staff  50 Feb  1 10:00 file_with_xattr.txt";
        let result = filter_ls(input, 0);

        assert!(!result.contains("total 8"));
        assert!(result.contains("dir_with_xattr/"));
        assert!(result.contains("file_with_xattr.txt"));
        assert!(!result.contains("user"));
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
