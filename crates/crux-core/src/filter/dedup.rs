/// Collapse consecutive identical lines into one.
pub fn apply_dedup(input: &str) -> String {
    let mut result = Vec::new();
    let mut prev: Option<&str> = None;
    for line in input.lines() {
        if prev != Some(line) {
            result.push(line);
        }
        prev = Some(line);
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_duplicates_collapsed() {
        assert_eq!(apply_dedup("a\na\nb\nb\nb\nc"), "a\nb\nc");
    }

    #[test]
    fn non_consecutive_duplicates_preserved() {
        assert_eq!(apply_dedup("a\nb\na\nb"), "a\nb\na\nb");
    }

    #[test]
    fn empty_input() {
        assert_eq!(apply_dedup(""), "");
    }

    #[test]
    fn all_identical_lines() {
        assert_eq!(apply_dedup("x\nx\nx\nx"), "x");
    }
}
