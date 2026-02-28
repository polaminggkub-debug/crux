use std::collections::HashMap;

use super::BuiltinFilterFn;

/// Register Firebase CLI handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("firebase deploy", filter_firebase_deploy as BuiltinFilterFn);
    m.insert("firebase", filter_firebase_generic as BuiltinFilterFn);
}

/// Filter `firebase deploy` output.
///
/// On success (exit_code 0): keep only "Deploy complete" / "release complete" checkmark
/// lines and any URL/Console lines. Drop all "i " info lines, "===" decorators, and
/// intermediate progress. Result is typically 2–4 lines.
///
/// On failure: keep lines that look like errors; drop info/progress noise.
pub fn filter_firebase_deploy(output: &str, exit_code: i32) -> String {
    let mut kept = Vec::new();

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
            // Keep "✔" lines only when they contain the final milestones.
            if trimmed.starts_with('✔') {
                if trimmed.contains("Deploy complete")
                    || trimmed.contains("release complete")
                    || trimmed.contains("Error")
                    || trimmed.contains("Warning")
                {
                    kept.push(trimmed.to_string());
                }
                // Drop other intermediate ✔ lines (file upload, version finalized, etc.)
                continue;
            }

            // Keep URL/Console summary lines.
            if trimmed.contains("URL:") || trimmed.contains("Console:") {
                kept.push(trimmed.to_string());
                continue;
            }

            // Keep explicit error/warning lines even on success exit.
            if trimmed.contains("Error") || trimmed.contains("Warning") {
                kept.push(trimmed.to_string());
            }
        } else {
            // On failure: keep error indicators, drop info/progress.
            if trimmed.contains("Error")
                || trimmed.contains("error")
                || trimmed.contains("ERR")
                || trimmed.starts_with('✖')
                || trimmed.contains("Failed")
            {
                kept.push(trimmed.to_string());
            }
        }
    }

    if kept.is_empty() {
        if exit_code == 0 {
            "Deploy completed.".to_string()
        } else {
            format!("Firebase deploy failed (exit code {exit_code}).")
        }
    } else {
        kept.join("\n")
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
    fn firebase_deploy_success() {
        let result = filter_firebase_deploy(DEPLOY_SUCCESS_OUTPUT, 0);

        // Must keep the final milestone and the URLs.
        assert!(
            result.contains("Deploy complete!"),
            "should keep Deploy complete"
        );
        assert!(result.contains("Hosting URL:"), "should keep Hosting URL");
        assert!(
            result.contains("Project Console:"),
            "should keep Console URL"
        );

        // Must drop info/progress noise.
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
        assert!(!result.contains("==="), "should drop decorator lines");

        // Result should be compact.
        let line_count = result.lines().count();
        assert!(
            line_count <= 4,
            "expected ≤4 lines on success, got {line_count}"
        );
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
}
