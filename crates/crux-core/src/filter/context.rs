use std::collections::HashMap;

/// Context passed through the filter pipeline stages.
///
/// Stages can read/write sections and variables to share data
/// (e.g. `section` stage populates `sections`, `template` stage reads them).
pub struct FilterContext {
    pub exit_code: i32,
    /// Named sections extracted by the `section` stage.
    pub sections: HashMap<String, Vec<String>>,
    /// Arbitrary variables for template interpolation.
    pub vars: HashMap<String, String>,
}

impl FilterContext {
    pub fn new(exit_code: i32) -> Self {
        Self {
            exit_code,
            sections: HashMap::new(),
            vars: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_context() {
        let ctx = FilterContext::new(0);
        assert_eq!(ctx.exit_code, 0);
        assert!(ctx.sections.is_empty());
        assert!(ctx.vars.is_empty());
    }

    #[test]
    fn context_with_data() {
        let mut ctx = FilterContext::new(1);
        ctx.sections
            .insert("errors".to_string(), vec!["err1".to_string()]);
        ctx.vars.insert("count".to_string(), "5".to_string());
        assert_eq!(ctx.sections["errors"], vec!["err1"]);
        assert_eq!(ctx.vars["count"], "5");
    }
}
