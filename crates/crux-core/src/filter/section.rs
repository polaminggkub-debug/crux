use regex::Regex;

use crate::config::types::SectionRule;

use super::context::FilterContext;

/// Extract or keep sections delimited by start/end regex patterns.
///
/// For each rule, lines between the start and end markers are collected
/// into `ctx.sections` keyed by `"section_N"`. If `rule.keep == Some(true)`,
/// the section lines remain in the output; otherwise they are removed.
pub fn apply_sections(input: &str, rules: &[SectionRule], ctx: &mut FilterContext) -> String {
    if rules.is_empty() {
        return input.to_string();
    }

    // Pre-compile regexes; skip rules with invalid patterns.
    let compiled: Vec<(usize, Regex, Option<Regex>, bool)> = rules
        .iter()
        .enumerate()
        .filter_map(|(i, rule)| {
            let start = Regex::new(&rule.start).ok()?;
            let end = rule.end.as_ref().and_then(|e| Regex::new(e).ok());
            let keep = rule.keep == Some(true);
            Some((i, start, end, keep))
        })
        .collect();

    let mut output_lines: Vec<String> = Vec::new();
    let mut active: Option<(usize, bool)> = None; // (rule_idx, keep)
    let mut section_buf: Vec<String> = Vec::new();

    for line in input.lines() {
        if let Some((rule_idx, keep)) = active {
            let (_, _, ref end_re, _) =
                compiled.iter().find(|(i, _, _, _)| *i == rule_idx).unwrap();
            let end_matched = end_re.as_ref().is_some_and(|re| re.is_match(line));

            if end_matched {
                section_buf.push(line.to_string());
                let key = format!("section_{}", rule_idx);
                ctx.sections.insert(key, section_buf.clone());
                if keep {
                    output_lines.append(&mut section_buf);
                } else {
                    section_buf.clear();
                }
                active = None;
            } else {
                section_buf.push(line.to_string());
            }
        } else {
            let mut matched = false;
            for &(idx, ref start_re, _, keep) in &compiled {
                if start_re.is_match(line) {
                    active = Some((idx, keep));
                    section_buf.push(line.to_string());
                    matched = true;
                    break;
                }
            }
            if !matched {
                output_lines.push(line.to_string());
            }
        }
    }

    // Handle open section at EOF (no end marker matched).
    if let Some((rule_idx, keep)) = active {
        let key = format!("section_{}", rule_idx);
        ctx.sections.insert(key, section_buf.clone());
        if keep {
            output_lines.extend(section_buf);
        }
    }

    output_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(start: &str, end: Option<&str>, keep: Option<bool>) -> SectionRule {
        SectionRule {
            start: start.to_string(),
            end: end.map(|s| s.to_string()),
            keep,
        }
    }

    #[test]
    fn extract_section_between_markers() {
        let input = "header\n[START]\nimportant\n[END]\nfooter";
        let rules = vec![rule(r"^\[START\]", Some(r"^\[END\]"), None)];
        let mut ctx = FilterContext::new(0);
        let out = apply_sections(input, &rules, &mut ctx);
        assert_eq!(out, "header\nfooter");
        assert_eq!(
            ctx.sections["section_0"],
            vec!["[START]", "important", "[END]"]
        );
    }

    #[test]
    fn keep_section_in_output() {
        let input = "before\n---errors---\nerr1\n---end---\nafter";
        let rules = vec![rule("---errors---", Some("---end---"), Some(true))];
        let mut ctx = FilterContext::new(0);
        let out = apply_sections(input, &rules, &mut ctx);
        assert_eq!(out, "before\n---errors---\nerr1\n---end---\nafter");
        assert!(ctx.sections.contains_key("section_0"));
    }

    #[test]
    fn section_without_end_captures_to_eof() {
        let input = "preamble\nSTART\nline1\nline2";
        let rules = vec![rule("^START$", None, None)];
        let mut ctx = FilterContext::new(0);
        let out = apply_sections(input, &rules, &mut ctx);
        assert_eq!(out, "preamble");
        assert_eq!(ctx.sections["section_0"], vec!["START", "line1", "line2"]);
    }

    #[test]
    fn multiple_sections() {
        let input = "a\n[S1]\nb\n[E1]\nc\n[S2]\nd\n[E2]\ne";
        let rules = vec![
            rule(r"^\[S1\]", Some(r"^\[E1\]"), None),
            rule(r"^\[S2\]", Some(r"^\[E2\]"), None),
        ];
        let mut ctx = FilterContext::new(0);
        let out = apply_sections(input, &rules, &mut ctx);
        assert_eq!(out, "a\nc\ne");
        assert!(ctx.sections.contains_key("section_0"));
        assert!(ctx.sections.contains_key("section_1"));
    }

    #[test]
    fn no_matching_section_returns_unchanged() {
        let input = "nothing special\njust lines";
        let rules = vec![rule("^NOMATCH$", Some("^END$"), None)];
        let mut ctx = FilterContext::new(0);
        let out = apply_sections(input, &rules, &mut ctx);
        assert_eq!(out, input);
        assert!(ctx.sections.is_empty());
    }
}
