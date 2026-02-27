use regex::Regex;

/// Strip ANSI escape codes from text.
pub fn strip_ansi(input: &str) -> String {
    // Matches CSI sequences, OSC sequences, and other common escape codes
    let re = Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][AB012]").unwrap();
    re.replace_all(input, "").into_owned()
}

/// Collapse consecutive blank lines to a single blank line.
pub fn collapse_blank_lines(input: &str) -> String {
    let mut result = Vec::new();
    let mut prev_blank = false;

    for line in input.lines() {
        let is_blank = line.trim().is_empty();
        if is_blank {
            if !prev_blank {
                result.push("");
            }
            prev_blank = true;
        } else {
            result.push(line);
            prev_blank = false;
        }
    }

    // Remove trailing blank line if present
    if result.last() == Some(&"") {
        result.pop();
    }

    result.join("\n")
}

/// Trim trailing whitespace from each line.
pub fn trim_trailing_whitespace(input: &str) -> String {
    input
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- strip_ansi tests --

    #[test]
    fn strip_ansi_removes_color_codes() {
        let input = "\x1b[31merror\x1b[0m: something failed";
        assert_eq!(strip_ansi(input), "error: something failed");
    }

    #[test]
    fn strip_ansi_removes_bold_and_underline() {
        let input = "\x1b[1mbold\x1b[0m and \x1b[4munderline\x1b[0m";
        assert_eq!(strip_ansi(input), "bold and underline");
    }

    #[test]
    fn strip_ansi_passthrough_plain_text() {
        let input = "no escape codes here";
        assert_eq!(strip_ansi(input), input);
    }

    #[test]
    fn strip_ansi_complex_sequences() {
        let input = "\x1b[38;5;196mred\x1b[0m \x1b[48;2;0;255;0mgreen bg\x1b[0m";
        assert_eq!(strip_ansi(input), "red green bg");
    }

    // -- collapse_blank_lines tests --

    #[test]
    fn collapse_multiple_blank_lines() {
        let input = "line1\n\n\n\nline2\n\n\nline3";
        assert_eq!(collapse_blank_lines(input), "line1\n\nline2\n\nline3");
    }

    #[test]
    fn collapse_no_blank_lines() {
        let input = "line1\nline2\nline3";
        assert_eq!(collapse_blank_lines(input), input);
    }

    #[test]
    fn collapse_single_blank_lines_unchanged() {
        let input = "line1\n\nline2\n\nline3";
        assert_eq!(collapse_blank_lines(input), input);
    }

    #[test]
    fn collapse_blank_lines_with_whitespace_only() {
        let input = "line1\n   \n  \n\nline2";
        assert_eq!(collapse_blank_lines(input), "line1\n\nline2");
    }

    // -- trim_trailing_whitespace tests --

    #[test]
    fn trim_trailing_spaces() {
        let input = "hello   \nworld  \nfoo";
        assert_eq!(trim_trailing_whitespace(input), "hello\nworld\nfoo");
    }

    #[test]
    fn trim_trailing_tabs() {
        let input = "hello\t\t\nworld\t";
        assert_eq!(trim_trailing_whitespace(input), "hello\nworld");
    }

    #[test]
    fn trim_preserves_leading_whitespace() {
        let input = "  hello  \n    world    ";
        assert_eq!(trim_trailing_whitespace(input), "  hello\n    world");
    }

    #[test]
    fn trim_no_trailing_whitespace_unchanged() {
        let input = "hello\nworld";
        assert_eq!(trim_trailing_whitespace(input), input);
    }
}
