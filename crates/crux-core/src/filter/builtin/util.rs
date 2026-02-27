use std::collections::HashMap;

use super::BuiltinFilterFn;

/// Register general utility command handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("curl", filter_curl as BuiltinFilterFn);
    m.insert("wget", filter_wget as BuiltinFilterFn);
    m.insert("wc", filter_wc as BuiltinFilterFn);
}

/// Filter curl output: strip progress bars and download stats.
/// Keep response body or error messages. Truncate body over 200 lines.
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

    let mut body_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip progress bar lines (contain --:--:-- or time patterns)
        if is_curl_progress_line(trimmed) {
            continue;
        }

        // Skip "  % Total    % Received" header
        if trimmed.starts_with("% Total") || trimmed.starts_with("Dload") {
            continue;
        }

        body_lines.push(line.to_string());
    }

    if body_lines.len() > 200 {
        let total = body_lines.len();
        body_lines.truncate(200);
        body_lines.push(format!("... ({} more lines, {} total)", total - 200, total));
    }

    if body_lines.is_empty() {
        "Empty response.".to_string()
    } else {
        body_lines.join("\n")
    }
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
        let lines: Vec<String> = (0..250).map(|i| format!("line {i}")).collect();
        let input = lines.join("\n");

        let result = filter_curl(&input, 0);
        assert!(result.contains("line 0"));
        assert!(result.contains("line 199"));
        assert!(result.contains("(50 more lines, 250 total)"));
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
}
