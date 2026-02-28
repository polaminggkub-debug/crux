use std::collections::HashMap;

use super::BuiltinFilterFn;

/// Register Firebase CLI handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("firebase deploy", filter_firebase_deploy as BuiltinFilterFn);
    m.insert(
        "firebase hosting:sites:list",
        filter_firebase_hosting_sites_list as BuiltinFilterFn,
    );
    m.insert("firebase", filter_firebase_generic as BuiltinFilterFn);
}

/// Filter `firebase deploy` output.
///
/// On success (exit_code 0): compress to just "Deploy complete!" + Hosting URL.
/// Drop Console URL, intermediate "release complete" lines, info/progress, decorators.
/// Result is typically 1–2 lines.
///
/// On failure: keep lines that look like errors; drop info/progress noise.
pub fn filter_firebase_deploy(output: &str, exit_code: i32) -> String {
    let mut hosting_url: Option<String> = None;
    let mut has_deploy_complete = false;
    let mut errors_warnings = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Always drop "===" decorator lines and "i " info/progress lines.
        if trimmed.starts_with("===") || trimmed.starts_with("i  ") || trimmed.starts_with("i ") {
            continue;
        }

        if exit_code == 0 {
            // Track if we saw "Deploy complete".
            if trimmed.contains("Deploy complete") {
                has_deploy_complete = true;
                continue;
            }

            // Extract the Hosting URL value.
            if trimmed.starts_with("Hosting URL:") {
                if let Some(url) = trimmed.strip_prefix("Hosting URL:") {
                    hosting_url = Some(url.trim().to_string());
                }
                continue;
            }

            // Drop Console URL — rarely needed.
            if trimmed.contains("Console:") {
                continue;
            }

            // Drop intermediate ✔ lines (release complete, file upload, version finalized).
            if trimmed.starts_with('✔') {
                // Still keep error/warning checkmark lines.
                if trimmed.contains("Error") || trimmed.contains("Warning") {
                    errors_warnings.push(trimmed.to_string());
                }
                continue;
            }

            // Keep explicit error/warning lines even on success exit.
            if trimmed.contains("Error") || trimmed.contains("Warning") {
                errors_warnings.push(trimmed.to_string());
            }
        } else {
            // On failure: keep error indicators, drop info/progress.
            if trimmed.contains("Error")
                || trimmed.contains("error")
                || trimmed.contains("ERR")
                || trimmed.starts_with('✖')
                || trimmed.contains("Failed")
            {
                errors_warnings.push(trimmed.to_string());
            }
        }
    }

    if exit_code == 0 {
        if has_deploy_complete {
            // Compact single-line format when we have both.
            if let Some(url) = &hosting_url {
                let mut result = format!("✔ Deploy complete! Hosting: {url}");
                if !errors_warnings.is_empty() {
                    result.push('\n');
                    result.push_str(&errors_warnings.join("\n"));
                }
                return result;
            }
            // Deploy complete but no hosting URL (e.g., functions-only deploy).
            let mut result = "✔ Deploy complete!".to_string();
            if !errors_warnings.is_empty() {
                result.push('\n');
                result.push_str(&errors_warnings.join("\n"));
            }
            return result;
        }
        if !errors_warnings.is_empty() {
            return errors_warnings.join("\n");
        }
        "Deploy completed.".to_string()
    } else {
        if !errors_warnings.is_empty() {
            return errors_warnings.join("\n");
        }
        format!("Firebase deploy failed (exit code {exit_code}).")
    }
}

/// Filter `firebase hosting:sites:list` output.
///
/// Extracts site names and default URLs from the table output.
/// Firebase CLI outputs a box-drawing table with Site ID, Default URL, and App ID columns.
/// Output: a count header plus one line per site with "site-id → url".
pub fn filter_firebase_hosting_sites_list(output: &str, exit_code: i32) -> String {
    if exit_code != 0 {
        // On failure, fall back to generic filtering.
        return filter_firebase_generic(output, exit_code);
    }

    let mut sites = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip empty, decorator, header, and info lines.
        if trimmed.is_empty()
            || trimmed.starts_with("i  ")
            || trimmed.starts_with("i ")
            || trimmed.starts_with("===")
            || trimmed.starts_with('┌')
            || trimmed.starts_with('├')
            || trimmed.starts_with('└')
            || trimmed.starts_with('─')
            || trimmed.starts_with('+')
        {
            continue;
        }

        // Parse table rows: │ col1 │ col2 │ col3 │
        if trimmed.starts_with('│') || trimmed.starts_with('|') {
            let sep = if trimmed.contains('│') { '│' } else { '|' };
            let cols: Vec<&str> = trimmed
                .split(sep)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if cols.len() >= 2 {
                let site_id = cols[0];
                let url = cols[1];

                // Skip the header row.
                if site_id.eq_ignore_ascii_case("Site ID")
                    || site_id.eq_ignore_ascii_case("site")
                    || site_id.contains("Site")
                {
                    continue;
                }

                // Skip separator-like rows (all dashes).
                if site_id.chars().all(|c| c == '-' || c == '─') {
                    continue;
                }

                if url.starts_with("http") {
                    sites.push(format!("{site_id} → {url}"));
                } else {
                    sites.push(site_id.to_string());
                }
            }
        }
    }

    if sites.is_empty() {
        // Fallback: try generic filter.
        filter_firebase_generic(output, exit_code)
    } else {
        let header = if sites.len() == 1 {
            "1 hosting site:".to_string()
        } else {
            format!("{} hosting sites:", sites.len())
        };
        let mut result = header;
        for site in &sites {
            result.push('\n');
            result.push_str("  ");
            result.push_str(site);
        }
        result
    }
}

/// Filter generic `firebase` subcommand output.
///
/// - Drop lines starting with "i " (info/progress).
/// - Drop "===" decorator lines.
/// - Keep lines starting with "✔", "✖", "Error", or "Warning".
/// - Keep any other substantive content (not pure whitespace).
/// - Truncate to 50 lines max.
pub fn filter_firebase_generic(output: &str, _exit_code: i32) -> String {
    let mut kept = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Drop info/progress and decorator lines.
        if trimmed.starts_with("i  ") || trimmed.starts_with("i ") || trimmed.starts_with("===") {
            continue;
        }

        kept.push(trimmed.to_string());

        if kept.len() >= 50 {
            break;
        }
    }

    if kept.is_empty() {
        output.trim().to_string()
    } else {
        kept.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEPLOY_SUCCESS_OUTPUT: &str = "\
=== Deploying to 'my-project'...

i  deploying hosting
i  hosting[my-project]: beginning deploy...
i  hosting[my-project]: found 47 files in dist
✔  hosting[my-project]: file upload complete
i  hosting[my-project]: finalizing version...
✔  hosting[my-project]: version finalized
i  hosting[my-project]: releasing new version...
✔  hosting[my-project]: release complete

✔  Deploy complete!

Project Console: https://console.firebase.google.com/project/my-project/overview
Hosting URL: https://my-project.web.app";

    #[test]
    fn firebase_deploy_success_compact() {
        let result = filter_firebase_deploy(DEPLOY_SUCCESS_OUTPUT, 0);

        // Should produce a single compact line with deploy status + hosting URL.
        assert_eq!(
            result,
            "✔ Deploy complete! Hosting: https://my-project.web.app"
        );

        // Must drop all noise.
        assert!(
            !result.contains("beginning deploy"),
            "should drop info lines"
        );
        assert!(!result.contains("found 47 files"), "should drop info lines");
        assert!(
            !result.contains("file upload complete"),
            "should drop intermediate ✔"
        );
        assert!(
            !result.contains("version finalized"),
            "should drop intermediate ✔"
        );
        assert!(
            !result.contains("release complete"),
            "should drop intermediate ✔"
        );
        assert!(!result.contains("==="), "should drop decorator lines");
        assert!(
            !result.contains("Console"),
            "should drop Console URL for brevity"
        );

        // Result should be just 1 line.
        let line_count = result.lines().count();
        assert_eq!(line_count, 1, "expected 1 line on clean success, got {line_count}");
    }

    #[test]
    fn firebase_deploy_success_no_hosting_url() {
        let input = "\
=== Deploying to 'my-project'...

i  deploying functions
i  functions: preparing codebase...
✔  functions: all functions deployed

✔  Deploy complete!

Project Console: https://console.firebase.google.com/project/my-project/overview";

        let result = filter_firebase_deploy(input, 0);
        assert_eq!(result, "✔ Deploy complete!");
    }

    #[test]
    fn firebase_deploy_success_with_warning() {
        let input = "\
=== Deploying to 'my-project'...

i  deploying hosting
✔  hosting[my-project]: release complete

✔  Deploy complete!

Warning: some deprecation notice
Hosting URL: https://my-project.web.app";

        let result = filter_firebase_deploy(input, 0);
        assert!(result.contains("Deploy complete!"), "should have deploy line");
        assert!(result.contains("Hosting:"), "should have hosting URL");
        assert!(
            result.contains("Warning: some deprecation notice"),
            "should keep warnings"
        );

        let line_count = result.lines().count();
        assert_eq!(line_count, 2, "expected 2 lines (deploy + warning), got {line_count}");
    }

    #[test]
    fn firebase_deploy_failure() {
        let input = "\
=== Deploying to 'my-project'...

i  deploying hosting
i  hosting[my-project]: beginning deploy...
Error: HTTP Error: 403, The caller does not have permission
✖  Deploy failed";

        let result = filter_firebase_deploy(input, 1);

        assert!(result.contains("Error:"), "should keep error line");
        assert!(
            result.contains("Deploy failed") || result.contains("✖"),
            "should keep failure marker"
        );
        assert!(
            !result.contains("beginning deploy"),
            "should drop info lines"
        );
        assert!(!result.contains("==="), "should drop decorator lines");
    }

    #[test]
    fn firebase_deploy_empty_success() {
        let result = filter_firebase_deploy("", 0);
        assert_eq!(result, "Deploy completed.");
    }

    #[test]
    fn firebase_deploy_empty_failure() {
        let result = filter_firebase_deploy("", 1);
        assert_eq!(result, "Firebase deploy failed (exit code 1).");
    }

    #[test]
    fn firebase_hosting_sites_list_table() {
        let input = "\
i  Preparing the list of your Firebase Hosting sites.
┌──────────────────┬────────────────────────────────────┬────────┐
│ Site ID          │ Default URL                        │ App ID │
├──────────────────┼────────────────────────────────────┼────────┤
│ my-app           │ https://my-app.web.app             │ --     │
├──────────────────┼────────────────────────────────────┼────────┤
│ my-app-staging   │ https://my-app-staging.web.app     │ --     │
├──────────────────┼────────────────────────────────────┼────────┤
│ my-app-dev       │ https://my-app-dev.web.app         │ --     │
└──────────────────┴────────────────────────────────────┴────────┘";

        let result = filter_firebase_hosting_sites_list(input, 0);

        assert!(result.starts_with("3 hosting sites:"), "should have count header");
        assert!(
            result.contains("my-app → https://my-app.web.app"),
            "should have first site"
        );
        assert!(
            result.contains("my-app-staging → https://my-app-staging.web.app"),
            "should have second site"
        );
        assert!(
            result.contains("my-app-dev → https://my-app-dev.web.app"),
            "should have third site"
        );

        // Should be compact — header + 3 sites = 4 lines.
        let line_count = result.lines().count();
        assert_eq!(line_count, 4, "expected 4 lines, got {line_count}");

        // Should drop all the box-drawing noise.
        assert!(!result.contains('┌'), "should drop table borders");
        assert!(!result.contains('│'), "should drop table separators");
        assert!(!result.contains("Preparing"), "should drop info lines");
    }

    #[test]
    fn firebase_hosting_sites_list_single() {
        let input = "\
┌──────────┬──────────────────────────────┬────────┐
│ Site ID  │ Default URL                  │ App ID │
├──────────┼──────────────────────────────┼────────┤
│ my-site  │ https://my-site.web.app      │ --     │
└──────────┴──────────────────────────────┴────────┘";

        let result = filter_firebase_hosting_sites_list(input, 0);
        assert!(result.starts_with("1 hosting site:"), "singular form for 1 site");
        assert!(result.contains("my-site → https://my-site.web.app"));
    }

    #[test]
    fn firebase_hosting_sites_list_failure() {
        let input = "\
i  Preparing the list...
Error: Failed to list hosting sites";

        let result = filter_firebase_hosting_sites_list(input, 1);
        // Falls back to generic filter on failure.
        assert!(result.contains("Error:"), "should keep error on failure");
        assert!(!result.contains("Preparing"), "should drop info lines");
    }

    #[test]
    fn firebase_generic_drops_info_lines() {
        let input = "\
i  Loading configuration...
i  Checking project settings...
i  Fetching data from Firebase...
Done.";

        let result = filter_firebase_generic(input, 0);

        assert!(
            !result.contains("Loading configuration"),
            "should drop i  lines"
        );
        assert!(!result.contains("Checking project"), "should drop i  lines");
        assert!(!result.contains("Fetching data"), "should drop i  lines");
        assert!(result.contains("Done."), "should keep substantive line");
    }

    #[test]
    fn firebase_generic_keeps_results() {
        let input = "\
i  Connecting to Firebase...
✔  Project linked successfully
✔  Configuration written to .firebaserc
i  Wrapping up...";

        let result = filter_firebase_generic(input, 0);

        assert!(result.contains("✔  Project linked successfully"));
        assert!(result.contains("✔  Configuration written to .firebaserc"));
        assert!(
            !result.contains("Connecting to Firebase"),
            "should drop info lines"
        );
        assert!(!result.contains("Wrapping up"), "should drop info lines");
    }

    #[test]
    fn firebase_deploy_real_world_savings() {
        // Simulate real-world output size similar to what was observed (720-820 bytes).
        let input = "\
=== Deploying to 'ssp-erp'...

i  deploying hosting
i  hosting[ssp-erp]: beginning deploy...
i  hosting[ssp-erp]: found 156 files in dist
i  hosting[ssp-erp]: uploading new files [2/156] (12%)
i  hosting[ssp-erp]: uploading new files [45/156] (29%)
i  hosting[ssp-erp]: uploading new files [120/156] (77%)
i  hosting[ssp-erp]: uploading new files [156/156] (100%)
✔  hosting[ssp-erp]: file upload complete
i  hosting[ssp-erp]: finalizing version...
✔  hosting[ssp-erp]: version finalized
i  hosting[ssp-erp]: releasing new version...
✔  hosting[ssp-erp]: release complete

✔  Deploy complete!

Project Console: https://console.firebase.google.com/project/ssp-erp/overview
Hosting URL: https://ssp-erp.web.app";

        let result = filter_firebase_deploy(input, 0);

        assert_eq!(result, "✔ Deploy complete! Hosting: https://ssp-erp.web.app");

        // Verify significant savings.
        let input_bytes = input.len();
        let output_bytes = result.len();
        let savings_pct = 100.0 * (1.0 - output_bytes as f64 / input_bytes as f64);
        assert!(
            savings_pct > 85.0,
            "expected >85% savings, got {savings_pct:.1}% ({input_bytes} → {output_bytes} bytes)"
        );
    }
}
