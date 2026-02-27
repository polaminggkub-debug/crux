use regex::Regex;

use super::context::FilterContext;

/// Interpolate `{var_name}` placeholders from context vars and sections.
///
/// Lookup order: `ctx.vars` first, then `ctx.sections` (joined with newlines).
/// Unknown variables are left as-is.
pub fn apply_template(template: &str, ctx: &FilterContext) -> String {
    let re = Regex::new(r"\{([a-zA-Z_][a-zA-Z0-9_]*)\}").expect("valid regex");
    re.replace_all(template, |caps: &regex::Captures| {
        let name = &caps[1];
        if let Some(val) = ctx.vars.get(name) {
            val.clone()
        } else if let Some(lines) = ctx.sections.get(name) {
            lines.join("\n")
        } else {
            caps[0].to_string()
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_var_replacement() {
        let mut ctx = FilterContext::new(0);
        ctx.vars.insert("name".into(), "crux".into());
        assert_eq!(apply_template("hello {name}!", &ctx), "hello crux!");
    }

    #[test]
    fn section_replacement_joins_lines() {
        let mut ctx = FilterContext::new(0);
        ctx.sections
            .insert("errors".into(), vec!["e1".into(), "e2".into()]);
        assert_eq!(apply_template("Errors:\n{errors}", &ctx), "Errors:\ne1\ne2");
    }

    #[test]
    fn unknown_var_left_as_is() {
        let ctx = FilterContext::new(0);
        assert_eq!(apply_template("{missing} text", &ctx), "{missing} text");
    }

    #[test]
    fn multiple_vars_in_one_template() {
        let mut ctx = FilterContext::new(0);
        ctx.vars.insert("a".into(), "1".into());
        ctx.vars.insert("b".into(), "2".into());
        ctx.sections
            .insert("c".into(), vec!["x".into(), "y".into()]);
        assert_eq!(apply_template("{a}+{b}={c}", &ctx), "1+2=x\ny");
    }
}
