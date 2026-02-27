use std::collections::HashMap;

use super::BuiltinFilterFn;

/// Register GitHub CLI handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("gh pr list", filter_gh_pr_list as BuiltinFilterFn);
    m.insert("gh pr view", filter_gh_pr_view as BuiltinFilterFn);
    m.insert("gh pr checks", filter_gh_pr_checks as BuiltinFilterFn);
    m.insert("gh issue list", filter_gh_issue_list as BuiltinFilterFn);
    m.insert("gh run list", filter_gh_run_list as BuiltinFilterFn);
    m.insert("gh api", filter_gh_api as BuiltinFilterFn);
}

/// Filter `gh pr list`: keep table rows (number, title, branch, status).
/// Drop header decoration. Limit to first 20 entries.
fn filter_gh_pr_list(output: &str, _exit_code: i32) -> String {
    filter_tabular_list(output, 20)
}

/// Filter `gh issue list`: keep number, title, labels. Limit 20 entries.
fn filter_gh_issue_list(output: &str, _exit_code: i32) -> String {
    filter_tabular_list(output, 20)
}

/// Shared logic for `gh pr list` and `gh issue list` — both produce tab-separated tables.
/// Keeps data rows, drops decoration and "Showing X of Y" footers.
fn filter_tabular_list(output: &str, max_rows: usize) -> String {
    let mut rows = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip lines that are pure decoration (dashes, equals, box-drawing)
        if is_decoration_line(trimmed) {
            continue;
        }

        // Skip the "Showing X of Y" footer
        if trimmed.starts_with("Showing ") && trimmed.contains(" of ") {
            continue;
        }

        rows.push(trimmed.to_string());

        if rows.len() >= max_rows {
            break;
        }
    }

    if rows.is_empty() {
        "No items found.".to_string()
    } else {
        rows.join("\n")
    }
}

/// Filter `gh pr view`: keep title, state, author, base<-head, body (first 5 lines).
/// Drop comments and reviews.
fn filter_gh_pr_view(output: &str, _exit_code: i32) -> String {
    let mut result = Vec::new();
    let mut body_lines_collected = 0;
    let mut in_body = false;
    let mut past_metadata = false;
    let mut seen_comments_section = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Stop at comments/reviews sections
        if trimmed == "-- Comments --"
            || trimmed == "-- Reviews --"
            || trimmed.starts_with("View this pull request")
        {
            seen_comments_section = true;
            continue;
        }

        if seen_comments_section {
            continue;
        }

        // Keep key metadata lines
        if trimmed.starts_with("title:")
            || trimmed.starts_with("state:")
            || trimmed.starts_with("author:")
            || trimmed.starts_with("number:")
            || trimmed.starts_with("url:")
        {
            result.push(trimmed.to_string());
            continue;
        }

        // Detect base<-head line (e.g. "main <- feature-branch")
        if trimmed.contains(" <- ") {
            result.push(trimmed.to_string());
            continue;
        }
        if trimmed.contains("into ") && trimmed.contains(" from ") {
            result.push(trimmed.to_string());
            continue;
        }

        // Detect the separator between header and body
        if is_decoration_line(trimmed) {
            if !past_metadata {
                past_metadata = true;
                in_body = true;
            }
            continue;
        }

        // Collect body lines (max 5)
        if in_body && body_lines_collected < 5 {
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
                body_lines_collected += 1;
            }
            continue;
        }

        // For non-metadata, non-body: if we haven't started body yet,
        // this could be the title or other header info from non-JSON mode
        if !past_metadata && !in_body && !trimmed.is_empty() && result.len() <= 6 {
            result.push(trimmed.to_string());
        }
    }

    if result.is_empty() {
        output.to_string()
    } else {
        result.join("\n")
    }
}

/// Filter `gh pr checks`: keep check name + status (pass/fail/pending).
/// Drop URLs and timing details.
fn filter_gh_pr_checks(output: &str, _exit_code: i32) -> String {
    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_decoration_line(trimmed) {
            continue;
        }

        // gh pr checks outputs tab-separated: name, status, elapsed, url
        // We want name + status only
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() >= 2 {
            let name = parts[0].trim();
            let status = parts[1].trim();
            lines.push(format!("{name}\t{status}"));
        } else {
            // Might be space-separated or a summary line
            let lower = trimmed.to_lowercase();
            if lower.contains("pass")
                || lower.contains("fail")
                || lower.contains("pending")
                || lower.contains("skipping")
                || lower.contains("success")
                || lower.contains("queued")
            {
                let cleaned = strip_urls(trimmed);
                lines.push(cleaned);
            } else if trimmed.starts_with("All checks")
                || trimmed.starts_with("Some checks")
                || trimmed.starts_with("0 failing")
                || trimmed.starts_with("0 pending")
            {
                lines.push(trimmed.to_string());
            }
        }
    }

    if lines.is_empty() {
        "No checks found.".to_string()
    } else {
        lines.join("\n")
    }
}

/// Filter `gh run list`: keep workflow name, status, branch, elapsed time. Drop IDs.
fn filter_gh_run_list(output: &str, _exit_code: i32) -> String {
    let mut rows = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_decoration_line(trimmed) {
            continue;
        }

        // gh run list outputs tab-separated columns:
        // STATUS  TITLE  WORKFLOW  BRANCH  EVENT  ID  ELAPSED  AGE
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() >= 5 {
            let status = parts[0].trim();
            let title = parts[1].trim();
            let workflow = parts[2].trim();
            let branch = parts[3].trim();
            // Skip EVENT (parts[4]) and ID (parts[5])
            let elapsed = if parts.len() >= 7 {
                parts[6].trim()
            } else {
                ""
            };

            let mut row = format!("{status}\t{title}\t{workflow}\t{branch}");
            if !elapsed.is_empty() {
                row.push('\t');
                row.push_str(elapsed);
            }
            rows.push(row);
        } else {
            // Fallback: strip numeric-only tokens that look like IDs (7+ digit numbers)
            let cleaned = strip_run_ids(trimmed);
            rows.push(cleaned);
        }

        if rows.len() >= 20 {
            break;
        }
    }

    if rows.is_empty() {
        "No workflow runs found.".to_string()
    } else {
        rows.join("\n")
    }
}

/// Filter `gh api`: JSON output passes through (already structured).
/// Non-JSON also passes through.
fn filter_gh_api(output: &str, _exit_code: i32) -> String {
    output.to_string()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if a line is purely decoration (dashes, equals, box-drawing chars).
fn is_decoration_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    line.chars()
        .all(|c| c == '-' || c == '=' || c == '+' || c == '|' || c == ' ' || c == '\t')
}

/// Strip URLs from a string (http:// or https://).
fn strip_urls(s: &str) -> String {
    s.split_whitespace()
        .filter(|token| !token.starts_with("http://") && !token.starts_with("https://"))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Strip tokens that look like run IDs (sequences of 7+ digits).
fn strip_run_ids(s: &str) -> String {
    s.split_whitespace()
        .filter(|token| !(token.len() >= 7 && token.chars().all(|c| c.is_ascii_digit())))
        .collect::<Vec<&str>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // gh pr list
    // -----------------------------------------------------------------------

    #[test]
    fn pr_list_keeps_data_rows() {
        let input = "#123\tFix login bug\tfix/login\tOPEN\n\
                      #124\tAdd dark mode\tfeature/dark\tOPEN\n\
                      #125\tBump deps\tchore/deps\tMERGED";
        let result = filter_gh_pr_list(input, 0);
        assert!(result.contains("#123"));
        assert!(result.contains("#125"));
        assert_eq!(result.lines().count(), 3);
    }

    #[test]
    fn pr_list_drops_decoration() {
        let input = "-------\n\
                      #123\tFix bug\tmain\tOPEN\n\
                      -------";
        let result = filter_gh_pr_list(input, 0);
        assert!(result.contains("#123"));
        assert!(!result.contains("---"));
    }

    #[test]
    fn pr_list_limits_to_20() {
        let mut lines = Vec::new();
        for i in 1..=30 {
            lines.push(format!("#{i}\tPR title {i}\tbranch-{i}\tOPEN"));
        }
        let input = lines.join("\n");
        let result = filter_gh_pr_list(&input, 0);
        assert_eq!(result.lines().count(), 20);
        assert!(result.contains("#1\t"));
        assert!(result.contains("#20\t"));
        assert!(!result.contains("#21\t"));
    }

    #[test]
    fn pr_list_empty() {
        let result = filter_gh_pr_list("", 0);
        assert_eq!(result, "No items found.");
    }

    #[test]
    fn pr_list_drops_showing_footer() {
        let input = "#1\tFix\tmain\tOPEN\n\
                      Showing 1 of 1 pull request";
        let result = filter_gh_pr_list(input, 0);
        assert!(result.contains("#1"));
        assert!(!result.contains("Showing"));
    }

    // -----------------------------------------------------------------------
    // gh pr view
    // -----------------------------------------------------------------------

    #[test]
    fn pr_view_keeps_metadata() {
        let input = "title:\tFix login bug\n\
                      state:\tOPEN\n\
                      author:\tjohndoe\n\
                      number:\t123\n\
                      url:\thttps://github.com/org/repo/pull/123\n\
                      --\n\
                      This PR fixes the login flow.\n\
                      It addresses issue #100.\n\
                      Also updates tests.\n\
                      -- Comments --\n\
                      reviewer: Looks good!\n\
                      reviewer2: LGTM";
        let result = filter_gh_pr_view(input, 0);
        assert!(result.contains("title:"));
        assert!(result.contains("state:"));
        assert!(result.contains("author:"));
        assert!(result.contains("This PR fixes"));
        assert!(!result.contains("Looks good"));
        assert!(!result.contains("LGTM"));
    }

    #[test]
    fn pr_view_limits_body() {
        let input = "title:\tBig PR\n\
                      state:\tOPEN\n\
                      --\n\
                      Line 1 of body\n\
                      Line 2 of body\n\
                      Line 3 of body\n\
                      Line 4 of body\n\
                      Line 5 of body\n\
                      Line 6 should be dropped\n\
                      Line 7 should be dropped";
        let result = filter_gh_pr_view(input, 0);
        assert!(result.contains("Line 5 of body"));
        assert!(!result.contains("Line 6"));
    }

    #[test]
    fn pr_view_drops_reviews() {
        let input = "title:\tSmall fix\n\
                      state:\tMERGED\n\
                      -- Reviews --\n\
                      APPROVED by reviewer1\n\
                      CHANGES_REQUESTED by reviewer2";
        let result = filter_gh_pr_view(input, 0);
        assert!(result.contains("title:"));
        assert!(!result.contains("APPROVED"));
        assert!(!result.contains("CHANGES_REQUESTED"));
    }

    #[test]
    fn pr_view_passthrough_on_empty() {
        let result = filter_gh_pr_view("", 0);
        assert_eq!(result, "");
    }

    // -----------------------------------------------------------------------
    // gh pr checks
    // -----------------------------------------------------------------------

    #[test]
    fn pr_checks_keeps_name_and_status() {
        let input = "CI / build\tpass\t2m30s\thttps://github.com/runs/123\n\
                      CI / lint\tfail\t1m10s\thttps://github.com/runs/124\n\
                      CI / test\tpending\t0s\thttps://github.com/runs/125";
        let result = filter_gh_pr_checks(input, 0);
        assert!(result.contains("CI / build\tpass"));
        assert!(result.contains("CI / lint\tfail"));
        assert!(result.contains("CI / test\tpending"));
        assert!(!result.contains("https://"));
    }

    #[test]
    fn pr_checks_keeps_summary() {
        let input = "All checks were successful\n\
                      0 failing, 0 pending, 3 passing";
        let result = filter_gh_pr_checks(input, 0);
        assert!(result.contains("All checks were successful"));
    }

    #[test]
    fn pr_checks_empty() {
        let result = filter_gh_pr_checks("", 0);
        assert_eq!(result, "No checks found.");
    }

    #[test]
    fn pr_checks_strips_urls_from_nontab_lines() {
        let input = "build  pass  https://github.com/actions/runs/999999999";
        let result = filter_gh_pr_checks(input, 0);
        assert!(result.contains("pass"));
        assert!(!result.contains("https://"));
    }

    // -----------------------------------------------------------------------
    // gh issue list
    // -----------------------------------------------------------------------

    #[test]
    fn issue_list_keeps_data() {
        let input = "#10\tBug: crash on start\tbug, critical\tOPEN\n\
                      #11\tFeature: dark mode\tenhancement\tOPEN\n\
                      #12\tDocs: update readme\tdocs\tCLOSED";
        let result = filter_gh_issue_list(input, 0);
        assert!(result.contains("#10"));
        assert!(result.contains("#12"));
        assert_eq!(result.lines().count(), 3);
    }

    #[test]
    fn issue_list_limits_to_20() {
        let mut lines = Vec::new();
        for i in 1..=25 {
            lines.push(format!("#{i}\tIssue {i}\tlabel\tOPEN"));
        }
        let input = lines.join("\n");
        let result = filter_gh_issue_list(&input, 0);
        assert_eq!(result.lines().count(), 20);
    }

    #[test]
    fn issue_list_empty() {
        let result = filter_gh_issue_list("", 0);
        assert_eq!(result, "No items found.");
    }

    // -----------------------------------------------------------------------
    // gh run list
    // -----------------------------------------------------------------------

    #[test]
    fn run_list_keeps_essentials_drops_ids() {
        let input = "completed\tUpdate deps\tCI\tmain\tpush\t1234567890\t3m20s\t2h ago";
        let result = filter_gh_run_list(input, 0);
        assert!(result.contains("completed"));
        assert!(result.contains("Update deps"));
        assert!(result.contains("CI"));
        assert!(result.contains("main"));
        assert!(result.contains("3m20s"));
        assert!(!result.contains("1234567890"));
    }

    #[test]
    fn run_list_multiple_rows() {
        let input = "completed\tBuild\tCI\tmain\tpush\t1111111111\t2m\t1h ago\n\
                      in_progress\tTest\tCI\tdev\tpush\t2222222222\t1m\t30m ago\n\
                      failure\tLint\tCI\tfix/bug\tpush\t3333333333\t5m\t2h ago";
        let result = filter_gh_run_list(input, 0);
        assert_eq!(result.lines().count(), 3);
        assert!(result.contains("failure"));
        assert!(result.contains("fix/bug"));
    }

    #[test]
    fn run_list_empty() {
        let result = filter_gh_run_list("", 0);
        assert_eq!(result, "No workflow runs found.");
    }

    #[test]
    fn run_list_strips_ids_from_nontab_lines() {
        let input = "completed Build CI main push 9876543210 3m 1h";
        let result = filter_gh_run_list(input, 0);
        assert!(!result.contains("9876543210"));
        assert!(result.contains("completed"));
    }

    // -----------------------------------------------------------------------
    // gh api
    // -----------------------------------------------------------------------

    #[test]
    fn api_passthrough_json() {
        let input = r#"{"login":"octocat","id":1,"name":"The Octocat"}"#;
        let result = filter_gh_api(input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn api_passthrough_text() {
        let input = "Not Found";
        let result = filter_gh_api(input, 1);
        assert_eq!(result, input);
    }

    #[test]
    fn api_passthrough_multiline_json() {
        let input = "[\n  {\"id\": 1},\n  {\"id\": 2}\n]";
        let result = filter_gh_api(input, 0);
        assert_eq!(result, input);
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    #[test]
    fn decoration_line_detection() {
        assert!(is_decoration_line("----------"));
        assert!(is_decoration_line("=========="));
        assert!(is_decoration_line("---+---+---"));
        assert!(!is_decoration_line("#123\tSome PR"));
        assert!(!is_decoration_line(""));
    }

    #[test]
    fn strip_urls_removes_http() {
        let input = "build pass https://github.com/runs/123 done";
        let result = strip_urls(input);
        assert_eq!(result, "build pass done");
    }

    #[test]
    fn strip_run_ids_removes_long_numbers() {
        let input = "completed Build CI main 1234567890 3m";
        let result = strip_run_ids(input);
        assert!(!result.contains("1234567890"));
        assert!(result.contains("completed"));
        assert!(result.contains("3m"));
    }
}
