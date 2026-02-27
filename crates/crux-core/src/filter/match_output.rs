use crate::config::types::MatchOutputRule;

pub fn apply_match_output(input: &str, rules: &[MatchOutputRule]) -> Option<String> {
    rules
        .iter()
        .find(|r| input.contains(&r.contains))
        .map(|r| r.template.clone().unwrap_or_else(|| r.contains.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(contains: &str, template: Option<&str>) -> MatchOutputRule {
        MatchOutputRule {
            contains: contains.to_string(),
            template: template.map(String::from),
        }
    }

    #[test]
    fn match_with_template_returns_template() {
        let rules = vec![rule("error", Some("Build failed"))];
        assert_eq!(
            apply_match_output("compile error found", &rules),
            Some("Build failed".into())
        );
    }

    #[test]
    fn match_without_template_returns_contains() {
        let rules = vec![rule("SUCCESS", None)];
        assert_eq!(
            apply_match_output("BUILD SUCCESS done", &rules),
            Some("SUCCESS".into())
        );
    }

    #[test]
    fn no_match_returns_none() {
        let rules = vec![rule("error", Some("bad"))];
        assert_eq!(apply_match_output("all good", &rules), None);
    }

    #[test]
    fn first_match_wins() {
        let rules = vec![rule("warn", Some("Warning")), rule("err", Some("Error"))];
        assert_eq!(
            apply_match_output("err and warn", &rules),
            Some("Warning".into())
        );
    }
}
