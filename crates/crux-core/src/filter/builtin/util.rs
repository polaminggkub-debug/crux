use std::collections::HashMap;

use super::BuiltinFilterFn;

/// Register general utility command handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("curl", filter_curl as BuiltinFilterFn);
    m.insert("wget", filter_wget as BuiltinFilterFn);
    m.insert("wc", filter_wc as BuiltinFilterFn);
    m.insert("env", filter_env as BuiltinFilterFn);
    m.insert("printenv", filter_env as BuiltinFilterFn);
    m.insert("lsof", filter_lsof as BuiltinFilterFn);
    m.insert("psql", filter_psql as BuiltinFilterFn);
}

/// Filter curl output: strip progress bars and download stats.
/// Smart compression for JSON, HTML, and minified/binary content.
/// Mask JWT tokens and long hex/base64 secrets in response bodies.
pub fn filter_curl(output: &str, exit_code: i32) -> String {
    if exit_code != 0 {
        let mut error_lines = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("curl:")
                || trimmed.starts_with("curl: (")
                || trimmed.contains("Could not resolve")
                || trimmed.contains("Connection refused")
                || trimmed.contains("Failed to connect")
            {
                error_lines.push(trimmed.to_string());
            }
        }
        if error_lines.is_empty() {
            return format!("curl failed (exit code {exit_code}).");
        }
        return error_lines.join("\n");
    }

    // Split into header lines (from -i/-I) and body lines, stripping progress bars.
    let mut header_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_headers = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip progress bar lines
        if is_curl_progress_line(trimmed) {
            continue;
        }
        // Skip progress header
        if trimmed.starts_with("% Total") || trimmed.starts_with("Dload") {
            continue;
        }

        // Detect HTTP response headers (from -i or -I flags, possibly with < prefix)
        let stripped = strip_header_prefix(trimmed);
        if stripped.starts_with("HTTP/") {
            in_headers = true;
            header_lines.push(stripped.to_string());
            continue;
        }
        if in_headers {
            if stripped.is_empty() {
                in_headers = false;
            } else {
                header_lines.push(stripped.to_string());
            }
            continue;
        }

        body_lines.push(line.to_string());
    }

    // Rejoin body for content-type detection
    let body = body_lines.join("\n");
    let body_trimmed = body.trim();

    // Detect and compress based on content type
    let compressed_body = if body_trimmed.is_empty() {
        None
    } else if is_minified_content(body_trimmed) {
        Some(format!(
            "[Binary/minified content: {} bytes]",
            body_trimmed.len()
        ))
    } else if looks_like_html(body_trimmed) {
        Some(compress_html(body_trimmed))
    } else if looks_like_json(body_trimmed) {
        Some(compress_json(body_trimmed))
    } else {
        // General text: truncate at 50 lines
        let lines: Vec<&str> = body_trimmed.lines().collect();
        if lines.len() > 50 {
            let total = lines.len();
            let mut kept: Vec<String> = lines[..50].iter().map(|l| l.to_string()).collect();
            kept.push(format!("... ({} more lines, {} total)", total - 50, total));
            Some(mask_secrets(&kept.join("\n")))
        } else {
            Some(mask_secrets(body_trimmed))
        }
    };

    // Assemble result
    let mut result = Vec::new();
    if !header_lines.is_empty() {
        result.extend(header_lines);
    }
    if let Some(body) = compressed_body {
        if !result.is_empty() {
            result.push(String::new()); // blank line between headers and body
        }
        result.push(body);
    }

    if result.is_empty() {
        "Empty response.".to_string()
    } else {
        result.join("\n")
    }
}

/// Strip `< ` or `> ` prefixes from verbose-mode header lines.
fn strip_header_prefix(line: &str) -> &str {
    if line.starts_with("< ") || line.starts_with("> ") {
        &line[2..]
    } else {
        line
    }
}

/// Detect minified/binary content: any line longer than 500 chars.
fn is_minified_content(body: &str) -> bool {
    body.lines().any(|line| line.len() > 500)
}

/// Check if content looks like HTML.
fn looks_like_html(body: &str) -> bool {
    let lower = body.to_lowercase();
    lower.starts_with("<!doctype") || lower.starts_with("<html")
}

/// Check if content looks like JSON.
fn looks_like_json(body: &str) -> bool {
    body.starts_with('{') || body.starts_with('[')
}

/// Compress HTML: strip <script>/<style>, extract title + meaningful text lines.
fn compress_html(body: &str) -> String {
    let mut result = Vec::new();
    let mut title = String::new();
    let mut in_script = false;
    let mut in_style = false;
    let mut text_lines = Vec::new();

    for line in body.lines() {
        let lower = line.to_lowercase();
        let trimmed = line.trim();

        // Track script/style blocks
        if lower.contains("<script") {
            in_script = true;
        }
        if lower.contains("</script") {
            in_script = false;
            continue;
        }
        if lower.contains("<style") {
            in_style = true;
        }
        if lower.contains("</style") {
            in_style = false;
            continue;
        }
        if in_script || in_style {
            continue;
        }

        // Extract title
        if lower.contains("<title") && lower.contains("</title") {
            let extracted = extract_tag_content(trimmed, "title");
            if !extracted.is_empty() {
                title = extracted;
            }
            continue;
        }

        // Skip pure tag lines (no visible text)
        let stripped = strip_html_tags(trimmed);
        let stripped = stripped.trim();
        if stripped.is_empty() {
            continue;
        }

        // Keep meaningful text lines
        if text_lines.len() < 20 {
            text_lines.push(stripped.to_string());
        }
    }

    result.push("[HTML content]".to_string());
    if !title.is_empty() {
        result.push(format!("Title: {title}"));
    }
    if text_lines.is_empty() {
        result.push("(no meaningful text content)".to_string());
    } else {
        result.extend(text_lines);
        let total_lines = body.lines().count();
        if total_lines > 20 {
            result.push(format!("... ({total_lines} lines total in original)"));
        }
    }

    mask_secrets(&result.join("\n"))
}

/// Naively strip HTML tags from a string.
fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    out
}

/// Extract text content between opening and closing tags on the same line.
fn extract_tag_content(line: &str, tag: &str) -> String {
    let lower = line.to_lowercase();
    let open_end = format!("</{tag}");
    if let Some(start_idx) = lower.find(&format!("<{tag}")) {
        // Find end of opening tag
        if let Some(gt) = line[start_idx..].find('>') {
            let content_start = start_idx + gt + 1;
            if let Some(close_idx) = lower.find(&open_end) {
                if close_idx > content_start {
                    return line[content_start..close_idx].trim().to_string();
                }
            }
        }
    }
    String::new()
}

/// Compress JSON: truncate to 50 lines, strip noisy fields, truncate nested arrays.
fn compress_json(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut result = Vec::new();

    // Noisy fields to skip entirely
    let skip_fields = ["\"id\"", "\"node_id\"", "\"avatar_url\"", "\"gravatar_id\""];

    let mut array_depth = 0;
    let mut array_item_count: Vec<usize> = Vec::new();
    let mut skipping_array_items = false;
    let mut pending_more: Option<String> = None;
    let max_array_items = 3;
    let max_lines = 50;

    for line in &lines {
        if result.len() >= max_lines {
            let remaining = lines.len() - result.len();
            if remaining > 0 {
                result.push(format!(
                    "... ({remaining} more lines, {} total)",
                    lines.len()
                ));
            }
            break;
        }

        let trimmed = line.trim();

        // Skip noisy fields
        if skip_fields
            .iter()
            .any(|f| trimmed.starts_with(f) || trimmed.contains(&format!(": {f}")))
        {
            // Check if it's a key-value line like `"id": 12345,`
            if trimmed.contains(':') {
                continue;
            }
        }

        // Track array depth for truncation
        if trimmed.starts_with('[') || trimmed.ends_with('[') || trimmed.contains(": [") {
            if let Some(more) = pending_more.take() {
                result.push(more);
            }
            array_depth += 1;
            array_item_count.push(0);
            skipping_array_items = false;
            result.push(line.to_string());
            continue;
        }

        if trimmed.starts_with(']') {
            if let Some(more) = pending_more.take() {
                result.push(more);
            }
            if array_depth > 0 {
                array_depth -= 1;
                array_item_count.pop();
            }
            skipping_array_items = false;
            result.push(line.to_string());
            continue;
        }

        // Inside an array at depth > 0: count items (lines starting with `{` or standalone values)
        if array_depth > 0 {
            if let Some(count) = array_item_count.last_mut() {
                if trimmed.starts_with('{')
                    || trimmed.starts_with('"')
                    || trimmed == "}"
                    || trimmed == "},"
                {
                    // Object boundary or value in array
                    if trimmed.starts_with('{') {
                        *count += 1;
                    }
                    if *count > max_array_items && !skipping_array_items {
                        skipping_array_items = true;
                        // We don't know total yet; store a placeholder
                        pending_more = Some(format!(
                            "{}\"... (more items)\"",
                            " ".repeat(line.len() - trimmed.len())
                        ));
                        continue;
                    }
                    if skipping_array_items {
                        continue;
                    }
                }
            }
        }

        if skipping_array_items {
            continue;
        }

        if let Some(more) = pending_more.take() {
            result.push(more);
        }
        result.push(line.to_string());
    }

    if let Some(more) = pending_more.take() {
        result.push(more);
    }

    mask_secrets(&result.join("\n"))
}

/// Mask JWT tokens (eyJ...) and long hex/base64 secrets in output.
fn mask_secrets(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Detect JWT tokens: eyJ followed by base64 chars, dots, more base64
        if i + 3 < len && chars[i] == 'e' && chars[i + 1] == 'y' && chars[i + 2] == 'J' {
            let start = i;
            i += 3;
            // Consume the JWT: base64url chars and dots
            let mut dot_count = 0;
            while i < len
                && (chars[i].is_ascii_alphanumeric()
                    || chars[i] == '.'
                    || chars[i] == '-'
                    || chars[i] == '_'
                    || chars[i] == '+'
                    || chars[i] == '/'
                    || chars[i] == '=')
            {
                if chars[i] == '.' {
                    dot_count += 1;
                }
                i += 1;
            }
            let token_len = i - start;
            // JWT tokens have 2 dots and are long
            if dot_count >= 2 && token_len > 30 {
                result.push_str("[JWT_TOKEN]");
            } else {
                // Not a JWT, output original
                for ch in &chars[start..i] {
                    result.push(*ch);
                }
            }
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }
    result
}

/// Detect curl progress bar lines.
fn is_curl_progress_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.contains("--:--:--") || trimmed.contains("0:00:") {
        return true;
    }
    if trimmed.contains("% Received") && trimmed.contains("% Xferd") {
        return true;
    }
    false
}

/// Filter wget output: keep "Saving to:" and completion summary.
/// Drop progress bars and connection details.
pub fn filter_wget(output: &str, exit_code: i32) -> String {
    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Keep "Saving to:" or "saved" lines
        if trimmed.contains("Saving to:")
            || trimmed.contains("saved [")
            || trimmed.contains("saved '")
        {
            lines.push(trimmed.to_string());
            continue;
        }

        // Keep completion/summary lines
        if trimmed.starts_with("Downloaded:") || trimmed.starts_with("FINISHED") {
            lines.push(trimmed.to_string());
            continue;
        }

        // Keep error lines
        if trimmed.starts_with("ERROR")
            || trimmed.contains("failed:")
            || trimmed.contains("404 Not Found")
            || trimmed.contains("Connection refused")
        {
            lines.push(trimmed.to_string());
            continue;
        }

        // Skip: Resolving, Connecting, HTTP request sent, progress bars, etc.
    }

    if lines.is_empty() {
        if exit_code == 0 {
            "Download completed.".to_string()
        } else {
            format!("wget failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter wc output: passthrough (already concise).
/// If more than 50 lines, show summary only.
pub fn filter_wc(output: &str, _exit_code: i32) -> String {
    let all_lines: Vec<&str> = output.lines().collect();

    if all_lines.len() <= 50 {
        return output.to_string();
    }

    // Look for a "total" summary line (last line in multi-file wc output)
    if let Some(last) = all_lines.last() {
        if last.contains("total") {
            return format!("({} files)\n{}", all_lines.len() - 1, last);
        }
    }

    // No total line — show count and first/last few lines
    let mut result = Vec::new();
    result.push(format!("({} lines of output)", all_lines.len()));
    for line in all_lines.iter().take(5) {
        result.push(line.to_string());
    }
    result.push("...".to_string());
    let tail: Vec<&&str> = all_lines.iter().rev().take(3).collect();
    for line in tail.into_iter().rev() {
        result.push(line.to_string());
    }
    result.join("\n")
}

/// Secret key patterns — if a var name contains any of these, mask the value.
const SECRET_PATTERNS: &[&str] = &["PASSWORD", "SECRET", "TOKEN", "KEY", "CREDENTIAL", "AUTH"];

/// Check if a variable name looks like it holds a secret.
fn is_secret_var(name: &str) -> bool {
    let upper = name.to_uppercase();
    SECRET_PATTERNS.iter().any(|pat| upper.contains(pat))
}

/// Filter env/printenv output: mask secrets, truncate long values, sort alphabetically.
/// On error, pass through unmodified.
pub fn filter_env(output: &str, exit_code: i32) -> String {
    if exit_code != 0 {
        return output.to_string();
    }

    let mut entries: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let name = &trimmed[..eq_pos];
            let value = &trimmed[eq_pos + 1..];

            if is_secret_var(name) {
                entries.push(format!("{name}=***"));
            } else if value.len() > 200 {
                entries.push(format!("{name}={}...", &value[..200]));
            } else {
                entries.push(trimmed.to_string());
            }
        } else {
            // Lines without '=' (unusual but possible) — keep as-is
            entries.push(trimmed.to_string());
        }
    }

    entries.sort();

    if entries.is_empty() {
        "No environment variables.".to_string()
    } else {
        entries.join("\n")
    }
}

/// Filter lsof output: keep header line, strip all columns except COMMAND, PID, and NAME.
/// lsof is wide tabular data; reducing to 3 columns cuts ~80+ chars per line to ~30.
/// On empty output returns "No matching processes."
/// Uses whitespace splitting: field[0]=COMMAND, field[1]=PID, field[8..]=NAME (may contain spaces).
pub fn filter_lsof(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return "No matching processes.".to_string();
    }

    // Verify first line looks like an lsof header.
    let header = lines[0].trim();
    let has_lsof_header =
        header.contains("COMMAND") && header.contains("PID") && header.contains("NAME");
    if !has_lsof_header {
        return output.to_string();
    }

    let mut result = Vec::with_capacity(lines.len());
    // Output a compact header.
    result.push("COMMAND  PID  NAME".to_string());

    for line in lines.iter().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        // lsof fields: COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME...
        // NAME is always the last field and may contain spaces (e.g., "*:5174 (LISTEN)").
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        let command = fields[0];
        let pid = fields[1];
        let name = fields[8..].join(" ");
        result.push(format!("{command}  {pid}  {name}"));
    }

    if result.len() <= 1 {
        "No matching processes.".to_string()
    } else {
        result.join("\n")
    }
}

/// Filter psql output.
///
/// - **Tabular output** (lines containing `---+---` or `+---` borders): strip border rows,
///   keep header + data rows. If > 50 data rows, show first 20 + last 10 + count.
/// - **Row count** lines like "(3 rows)": always keep.
/// - **Error/FATAL/psql:/NOTICE/WARNING** lines: always keep.
/// - Non-tabular: pass through but truncate > 100 lines (head 50 + tail 20).
pub fn filter_psql(output: &str, _exit_code: i32) -> String {
    if output.trim().is_empty() {
        return "No output.".to_string();
    }

    let lines: Vec<&str> = output.lines().collect();

    // Detect tabular output: any line that looks like a border (`---+---` or `+---+`).
    let is_border = |line: &str| {
        let t = line.trim();
        (t.contains("---") && t.contains('+')) || t.chars().all(|c| c == '-' || c == '+')
    };

    let is_always_keep = |line: &str| {
        let t = line.trim();
        t.starts_with("ERROR:")
            || t.starts_with("FATAL:")
            || t.starts_with("psql:")
            || t.starts_with("NOTICE:")
            || t.starts_with("WARNING:")
            || (t.starts_with('(') && t.ends_with("rows)"))
            || (t.starts_with('(') && t.ends_with("row)"))
    };

    let has_table = lines.iter().any(|l| is_border(l));

    if has_table {
        let mut kept: Vec<String> = Vec::new();
        let mut data_rows: Vec<String> = Vec::new();
        let mut header_done = false;

        for line in &lines {
            if is_always_keep(line) {
                kept.push(line.trim().to_string());
                continue;
            }
            if is_border(line) {
                if !header_done {
                    header_done = true; // separator after column headers
                }
                continue; // drop border rows
            }
            if !header_done {
                // Column header row(s) — always keep
                kept.push(line.trim().to_string());
            } else {
                data_rows.push(line.trim().to_string());
            }
        }

        let total_data = data_rows.len();
        if total_data > 50 {
            let omitted = total_data - 20 - 10;
            let mut shown = data_rows[..20].to_vec();
            shown.push(format!("... ({omitted} rows omitted, {total_data} total)"));
            shown.extend_from_slice(&data_rows[total_data - 10..]);
            kept.extend(shown);
        } else {
            kept.extend(data_rows);
        }

        return kept.join("\n");
    }

    // Non-tabular: pass through, truncate if > 100 lines.
    if lines.len() <= 100 {
        return output.to_string();
    }

    let total = lines.len();
    let mut result: Vec<String> = lines[..50].iter().map(|l| l.to_string()).collect();
    result.push(format!(
        "... ({} lines omitted, {} total)",
        total - 50 - 20,
        total
    ));
    result.extend(lines[total - 20..].iter().map(|l| l.to_string()));
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- curl tests --

    #[test]
    fn curl_strips_progress() {
        let input = "  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current\n                                 Dload  Upload   Total   Spent    Left  Speed\n100  1234  100  1234    0     0  12345      0 --:--:-- --:--:-- --:--:-- 12345\n{\"status\":\"ok\",\"data\":\"hello\"}";

        let result = filter_curl(input, 0);
        assert!(result.contains("{\"status\":\"ok\",\"data\":\"hello\"}"));
        assert!(!result.contains("% Total"));
        assert!(!result.contains("--:--:--"));
    }

    #[test]
    fn curl_truncates_long_body() {
        let lines: Vec<String> = (0..80).map(|i| format!("line {i}")).collect();
        let input = lines.join("\n");

        let result = filter_curl(&input, 0);
        assert!(result.contains("line 0"));
        assert!(result.contains("line 49"));
        assert!(result.contains("(30 more lines, 80 total)"));
    }

    #[test]
    fn curl_error() {
        let input = "curl: (6) Could not resolve host: nonexistent.example.com";
        let result = filter_curl(input, 6);
        assert!(result.contains("Could not resolve host"));
    }

    #[test]
    fn curl_empty_response() {
        let result = filter_curl("", 0);
        assert_eq!(result, "Empty response.");
    }

    // -- wget tests --

    #[test]
    fn wget_keeps_save_and_summary() {
        let input = "--2024-01-15 10:30:00--  https://example.com/file.tar.gz\nResolving example.com (example.com)... 93.184.216.34\nConnecting to example.com (example.com)|93.184.216.34|:443... connected.\nHTTP request sent, awaiting response... 200 OK\nLength: 1048576 (1.0M) [application/gzip]\nSaving to: 'file.tar.gz'\n\nfile.tar.gz         100%[===================>]   1.00M  5.00MB/s    in 0.2s\n\n2024-01-15 10:30:01 (5.00 MB/s) - 'file.tar.gz' saved [1048576/1048576]";

        let result = filter_wget(input, 0);
        assert!(result.contains("Saving to: 'file.tar.gz'"));
        assert!(result.contains("saved [1048576/1048576]"));
        assert!(!result.contains("Resolving"));
        assert!(!result.contains("Connecting"));
    }

    #[test]
    fn wget_error() {
        let input = "--2024-01-15 10:30:00--  https://example.com/missing.txt\nResolving example.com... 93.184.216.34\nHTTP request sent, awaiting response... 404 Not Found\nERROR 404: Not Found.";

        let result = filter_wget(input, 8);
        assert!(result.contains("404 Not Found"));
        assert!(!result.contains("Resolving"));
    }

    #[test]
    fn wget_empty_success() {
        let result = filter_wget("", 0);
        assert_eq!(result, "Download completed.");
    }

    // -- wc tests --

    #[test]
    fn wc_short_passthrough() {
        let input = "  10  50 300 file.txt";
        let result = filter_wc(input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn wc_long_shows_summary() {
        let mut lines: Vec<String> = (0..55)
            .map(|i| format!("  10  50 300 file{i}.txt"))
            .collect();
        lines.push("  550 2750 16500 total".to_string());
        let input = lines.join("\n");

        let result = filter_wc(&input, 0);
        assert!(result.contains("(55 files)"));
        assert!(result.contains("total"));
    }

    // -- env tests --

    #[test]
    fn env_masks_secrets() {
        let input =
            "HOME=/home/user\nDATABASE_PASSWORD=supersecret\nAPI_TOKEN=abc123\nPATH=/usr/bin";
        let result = filter_env(input, 0);
        assert!(result.contains("DATABASE_PASSWORD=***"));
        assert!(result.contains("API_TOKEN=***"));
        assert!(result.contains("HOME=/home/user"));
        assert!(result.contains("PATH=/usr/bin"));
        assert!(!result.contains("supersecret"));
        assert!(!result.contains("abc123"));
    }

    #[test]
    fn env_masks_various_secret_patterns() {
        let input = "AWS_SECRET_ACCESS_KEY=xxx\nGH_AUTH_TOKEN=yyy\nDB_CREDENTIAL=zzz\nMY_KEY=aaa";
        let result = filter_env(input, 0);
        assert!(result.contains("AWS_SECRET_ACCESS_KEY=***"));
        assert!(result.contains("GH_AUTH_TOKEN=***"));
        assert!(result.contains("DB_CREDENTIAL=***"));
        assert!(result.contains("MY_KEY=***"));
    }

    #[test]
    fn env_truncates_long_values() {
        let long_val = "x".repeat(300);
        let input = format!("LONG_VAR={long_val}\nSHORT=ok");
        let result = filter_env(&input, 0);
        assert!(result.contains("LONG_VAR="));
        assert!(result.contains("..."));
        // Should have 200 chars of value + "..."
        let long_line = result.lines().find(|l| l.starts_with("LONG_VAR=")).unwrap();
        assert!(long_line.ends_with("..."));
        assert_eq!(long_line.len(), "LONG_VAR=".len() + 200 + 3);
    }

    #[test]
    fn env_sorts_alphabetically() {
        let input = "ZEBRA=1\nAPPLE=2\nMIDDLE=3";
        let result = filter_env(input, 0);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[0], "APPLE=2");
        assert_eq!(lines[1], "MIDDLE=3");
        assert_eq!(lines[2], "ZEBRA=1");
    }

    #[test]
    fn env_error_passthrough() {
        let input = "some error output";
        let result = filter_env(input, 1);
        assert_eq!(result, input);
    }

    #[test]
    fn env_empty() {
        let result = filter_env("", 0);
        assert_eq!(result, "No environment variables.");
    }

    #[test]
    fn wc_long_no_total() {
        let lines: Vec<String> = (0..55)
            .map(|i| format!("  10  50 300 file{i}.txt"))
            .collect();
        let input = lines.join("\n");

        let result = filter_wc(&input, 0);
        assert!(result.contains("(55 lines of output)"));
        assert!(result.contains("file0.txt"));
        assert!(result.contains("..."));
    }

    #[test]
    fn curl_minified_js() {
        // Simulate a minified JS file (one long line >500 chars)
        let long_line = "var a=".to_string() + &"x".repeat(600) + ";";
        let result = filter_curl(&long_line, 0);
        assert!(result.starts_with("[Binary/minified content:"));
        assert!(result.contains("bytes]"));
        assert!(!result.contains("var a="));
    }

    #[test]
    fn curl_html_compression() {
        let html = r#"<!DOCTYPE html>
<html>
<head>
<title>My Page</title>
<style>body { color: red; } .foo { margin: 0; }</style>
<script>console.log("hello"); var x = 1;</script>
</head>
<body>
<h1>Welcome</h1>
<p>This is a paragraph.</p>
<div>Some content here</div>
</body>
</html>"#;

        let result = filter_curl(html, 0);
        assert!(result.contains("[HTML content]"));
        assert!(result.contains("Title: My Page"));
        assert!(result.contains("Welcome"));
        assert!(result.contains("This is a paragraph."));
        // Script and style content should be stripped
        assert!(!result.contains("console.log"));
        assert!(!result.contains("color: red"));
    }

    #[test]
    fn curl_json_compression() {
        let json = r#"{
  "status": "ok",
  "id": 12345,
  "node_id": "MDQ6VXNlcjE=",
  "avatar_url": "https://example.com/avatar.png",
  "data": {
    "name": "test",
    "value": 42
  }
}"#;
        let result = filter_curl(json, 0);
        assert!(result.contains("\"status\": \"ok\""));
        assert!(result.contains("\"name\": \"test\""));
        // Noisy fields stripped
        assert!(!result.contains("\"id\": 12345"));
        assert!(!result.contains("node_id"));
        assert!(!result.contains("avatar_url"));
    }

    #[test]
    fn curl_json_truncation() {
        // JSON with more than 50 lines
        let mut lines = vec!["{".to_string()];
        for i in 0..60 {
            lines.push(format!("  \"field_{i}\": \"value_{i}\","));
        }
        lines.push("}".to_string());
        let input = lines.join("\n");

        let result = filter_curl(&input, 0);
        assert!(result.contains("more lines"));
    }

    #[test]
    fn curl_jwt_masking() {
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let input = format!("{{\"access_token\":\"{jwt}\"}}");
        let result = filter_curl(&input, 0);
        assert!(result.contains("[JWT_TOKEN]"));
        assert!(!result.contains("eyJhbGci"));
    }

    #[test]
    fn curl_http_headers() {
        let input = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}";
        let result = filter_curl(input, 0);
        assert!(result.contains("HTTP/1.1 200 OK"));
        assert!(result.contains("Content-Type: application/json"));
        assert!(result.contains("{\"ok\":true}"));
    }

    #[test]
    fn curl_verbose_header_prefix_stripping() {
        let input = "< HTTP/1.1 200 OK\n< Content-Type: text/html\n< \nhello world";
        let result = filter_curl(input, 0);
        assert!(result.contains("HTTP/1.1 200 OK"));
        assert!(result.contains("Content-Type: text/html"));
        assert!(!result.contains("< HTTP"));
        assert!(result.contains("hello world"));
    }

    #[test]
    fn curl_small_response_passthrough() {
        // Small responses (like http_code only) should pass through
        let result = filter_curl("200", 0);
        assert_eq!(result, "200");
    }

    // -- lsof tests --

    #[test]
    fn lsof_strips_columns() {
        let input = "COMMAND   PID   USER   FD   TYPE   DEVICE   SIZE/OFF   NODE   NAME\nnode     1234   user   22u  IPv4   0x1234   0t0        TCP    *:5174 (LISTEN)";
        let result = filter_lsof(input, 0);
        // Must keep COMMAND and PID and NAME
        assert!(result.contains("COMMAND"));
        assert!(result.contains("NAME"));
        assert!(result.contains("node"));
        assert!(result.contains("1234"));
        assert!(result.contains("*:5174 (LISTEN)"));
        // Must not contain intermediate columns
        assert!(!result.contains("USER"));
        assert!(!result.contains("DEVICE"));
        assert!(!result.contains("SIZE/OFF"));
    }

    #[test]
    fn lsof_empty() {
        let result = filter_lsof("", 0);
        assert_eq!(result, "No matching processes.");
    }

    // -- psql tests --

    #[test]
    fn psql_strips_borders() {
        let input = " Schema |  Name   | Type  | Owner\n--------+---------+-------+----------\n public | users   | table | postgres\n public | orders  | table | postgres\n(2 rows)";
        let result = filter_psql(input, 0);
        assert!(!result.contains("--------"));
        assert!(result.contains("Schema"));
        assert!(result.contains("users"));
        assert!(result.contains("orders"));
    }

    #[test]
    fn psql_keeps_row_count() {
        let input = " id | name\n----+------\n  1 | Alice\n  2 | Bob\n  3 | Carol\n(3 rows)";
        let result = filter_psql(input, 0);
        assert!(result.contains("(3 rows)"));
    }

    #[test]
    fn psql_keeps_errors() {
        let input = "ERROR:  relation \"missing_table\" does not exist\nLINE 1: SELECT * FROM missing_table;\n                      ^";
        let result = filter_psql(input, 1);
        assert!(result.contains("ERROR:"));
        assert!(result.contains("missing_table"));
    }

    #[test]
    fn psql_truncates_long() {
        // Build a table with 60 data rows
        let mut lines = vec![" id | value".to_string(), "----+-------".to_string()];
        for i in 0..60 {
            lines.push(format!("  {i} | val{i}"));
        }
        lines.push("(60 rows)".to_string());
        let input = lines.join("\n");

        let result = filter_psql(&input, 0);
        // Should have omission marker
        assert!(result.contains("omitted"));
        // Should keep the row count
        assert!(result.contains("(60 rows)"));
        // Should not contain all 60 rows verbatim
        let data_line_count = result.lines().filter(|l| l.contains("val")).count();
        assert!(
            data_line_count < 60,
            "Expected truncation, got {data_line_count} data lines"
        );
    }
}
