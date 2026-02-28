use regex::Regex;
use std::sync::LazyLock;

/// Pre-compiled regex for ANSI escape codes (CSI sequences, OSC, charset).
static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][AB012]|\x1b\[\?[0-9;]*[hl]")
        .unwrap()
});

/// Pre-compiled regex for progress bar patterns.
static PROGRESS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(\[[\s=>\-#\.]*\]|\d{1,3}%|.*\d{1,3}\s*%\s*\|[█▓░▏▎▍▌▋▊▉\s]*\|)").unwrap()
});

/// Pre-compiled regex for download progress lines.
static DOWNLOAD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(downloading|fetching|pulling)\s*\(?\d+/\d+\)?\s*\.{0,3}").unwrap()
});

/// Spinner characters used in CLI progress indicators.
const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Pre-compiled regex for hint/note lines in post-filter.
static HINT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)^\s*(\(use ".*" to .*\)|hint:\s)"#).unwrap());

/// Pre-compiled regex for standalone note lines (not in error context).
static NOTE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s*note:\s").unwrap());

/// Returns true if a line is a progress/spinner line that should be removed.
fn is_progress_line(line: &str) -> bool {
    // Check for spinner characters at start of trimmed line
    let trimmed = line.trim();
    if let Some(ch) = trimmed.chars().next() {
        if SPINNER_CHARS.contains(&ch) {
            return true;
        }
    }

    // Check for lines dominated by progress bar chars (━, ▓, ░)
    if trimmed.contains('━') || trimmed.contains('▓') || trimmed.contains('░') {
        let bar_chars: usize = trimmed
            .chars()
            .filter(|c| {
                matches!(
                    c,
                    '━' | '▓' | '░' | '█' | '▏' | '▎' | '▍' | '▌' | '▋' | '▊' | '▉'
                )
            })
            .count();
        if bar_chars > 3 {
            return true;
        }
    }

    PROGRESS_RE.is_match(line) || DOWNLOAD_RE.is_match(line)
}

/// Pre-filter: runs BEFORE builtin matching.
///
/// - Strips ANSI escape codes
/// - Removes progress bar / spinner / download progress lines
pub fn pre_filter(output: &str) -> String {
    // Strip ANSI first so progress detection works on clean text
    let stripped = ANSI_RE.replace_all(output, "");

    stripped
        .lines()
        .filter(|line| !is_progress_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Post-filter: runs AFTER builtin filtering.
///
/// - Collapses 3+ consecutive blank lines to 1 blank line
/// - Removes hint/note lines
pub fn post_filter(output: &str) -> String {
    let mut result = Vec::new();
    let mut consecutive_blanks: usize = 0;
    let mut in_error_context = false;

    for line in output.lines() {
        let trimmed = line.trim();
        let is_blank = trimmed.is_empty();

        if is_blank {
            consecutive_blanks += 1;
            if consecutive_blanks <= 2 {
                result.push(line);
            } else if consecutive_blanks == 3 {
                // Replace 3+ blanks with single blank (already have 2, keep just 1)
                // Remove the extra blank we added
                while result.last().is_some_and(|l: &&str| l.trim().is_empty()) {
                    result.pop();
                }
                result.push("");
            }
            // 4+ blanks: skip entirely (already collapsed)
            in_error_context = false;
            continue;
        }

        consecutive_blanks = 0;

        // Track error context (error lines often precede relevant notes)
        if trimmed.starts_with("error") || trimmed.starts_with("Error") {
            in_error_context = true;
        }

        // Skip hint lines
        if HINT_RE.is_match(line) {
            continue;
        }

        // Skip note lines only when NOT in error context
        if !in_error_context && NOTE_RE.is_match(line) {
            continue;
        }

        result.push(line);
    }

    // Remove trailing blank lines
    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ANSI stripping tests --

    #[test]
    fn pre_filter_strips_color_codes() {
        let input = "\x1b[31merror\x1b[0m: something failed";
        assert_eq!(pre_filter(input), "error: something failed");
    }

    #[test]
    fn pre_filter_strips_bold_underline() {
        let input = "\x1b[1mbold\x1b[0m and \x1b[4munderline\x1b[0m";
        assert_eq!(pre_filter(input), "bold and underline");
    }

    #[test]
    fn pre_filter_strips_256_color() {
        let input = "\x1b[38;5;196mred\x1b[0m";
        assert_eq!(pre_filter(input), "red");
    }

    #[test]
    fn pre_filter_strips_cursor_movement() {
        let input = "\x1b[2Kline cleared\x1b[1Aup one";
        assert_eq!(pre_filter(input), "line clearedup one");
    }

    #[test]
    fn pre_filter_strips_private_mode() {
        let input = "\x1b[?25lhidden cursor\x1b[?25h";
        assert_eq!(pre_filter(input), "hidden cursor");
    }

    // -- Progress bar removal --

    #[test]
    fn pre_filter_removes_bracket_progress() {
        let input = "Building...\n[====>     ] 40%\nDone!";
        assert_eq!(pre_filter(input), "Building...\nDone!");
    }

    #[test]
    fn pre_filter_removes_percentage_line() {
        let input = "Step 1\n  50%\nStep 2";
        assert_eq!(pre_filter(input), "Step 1\nStep 2");
    }

    #[test]
    fn pre_filter_removes_spinner_lines() {
        let input = "Loading\n⠋ Installing packages...\n⠙ Still going...\nDone";
        assert_eq!(pre_filter(input), "Loading\nDone");
    }

    #[test]
    fn pre_filter_removes_unicode_bar_lines() {
        let input = "Progress:\n━━━━━━━━━━━━━━━━\nComplete";
        assert_eq!(pre_filter(input), "Progress:\nComplete");
    }

    #[test]
    fn pre_filter_removes_block_progress_bar() {
        let input = "Downloading:\n  50% |████     |\nFinished";
        assert_eq!(pre_filter(input), "Downloading:\nFinished");
    }

    // -- Download progress --

    #[test]
    fn pre_filter_removes_download_progress() {
        let input = "Starting\nDownloading (3/10)...\nFetching (1/5)\nDone";
        assert_eq!(pre_filter(input), "Starting\nDone");
    }

    // -- Edge cases --

    #[test]
    fn pre_filter_empty_input() {
        assert_eq!(pre_filter(""), "");
    }

    #[test]
    fn pre_filter_clean_input_unchanged() {
        let input = "clean line 1\nclean line 2";
        assert_eq!(pre_filter(input), input);
    }

    #[test]
    fn pre_filter_combined_ansi_and_progress() {
        let input = "\x1b[32mBuilding\x1b[0m\n\x1b[33m[====>  ]\x1b[0m\n\x1b[32mDone\x1b[0m";
        assert_eq!(pre_filter(input), "Building\nDone");
    }

    // -- Post-filter: blank line collapsing --

    #[test]
    fn post_filter_collapses_3_plus_blanks() {
        let input = "line1\n\n\n\n\nline2";
        assert_eq!(post_filter(input), "line1\n\nline2");
    }

    #[test]
    fn post_filter_keeps_2_blanks() {
        let input = "line1\n\nline2";
        assert_eq!(post_filter(input), "line1\n\nline2");
    }

    #[test]
    fn post_filter_keeps_single_blank() {
        let input = "line1\n\nline2";
        assert_eq!(post_filter(input), input);
    }

    // -- Post-filter: hint/note suppression --

    #[test]
    fn post_filter_removes_hint_lines() {
        let input = "error: failed\nhint: try again\nfatal: abort";
        assert_eq!(post_filter(input), "error: failed\nfatal: abort");
    }

    #[test]
    fn post_filter_removes_use_hint_pattern() {
        let input = "Changes not staged:\n(use \"git add\" to update)\n\tmodified: file.rs";
        assert_eq!(
            post_filter(input),
            "Changes not staged:\n\tmodified: file.rs"
        );
    }

    #[test]
    fn post_filter_removes_note_lines() {
        let input = "warning: unused variable\nnote: consider using _\nother line";
        assert_eq!(post_filter(input), "warning: unused variable\nother line");
    }

    #[test]
    fn post_filter_keeps_note_in_error_context() {
        let input = "error[E0308]: mismatched types\nnote: expected `u32`, found `String`";
        assert_eq!(post_filter(input), input);
    }

    // -- Post-filter: edge cases --

    #[test]
    fn post_filter_empty_input() {
        assert_eq!(post_filter(""), "");
    }

    #[test]
    fn post_filter_clean_input_unchanged() {
        let input = "line1\nline2\nline3";
        assert_eq!(post_filter(input), input);
    }

    // -- Combined scenarios --

    #[test]
    fn pre_then_post_full_pipeline() {
        let input = "\x1b[31merror: bad\x1b[0m\nhint: fix it\n[====>  ]\n\n\n\n\nok";
        let result = post_filter(&pre_filter(input));
        assert_eq!(result, "error: bad\n\nok");
    }

    #[test]
    fn pre_filter_preserves_meaningful_content() {
        let input = "Compiling my-crate v0.1.0\n    Finished dev [unoptimized] target(s) in 2.5s";
        assert_eq!(pre_filter(input), input);
    }

    #[test]
    fn post_filter_trailing_blanks_removed() {
        let input = "content\n\n\n\n";
        assert_eq!(post_filter(input), "content");
    }
}
