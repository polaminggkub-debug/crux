use std::collections::HashMap;

pub mod cargo;
pub mod git;
pub mod npm;

/// A builtin filter function: takes raw output + exit code, returns compressed output.
pub type BuiltinFilterFn = fn(output: &str, exit_code: i32) -> String;

/// Registry of all builtin filters.
pub fn registry() -> HashMap<&'static str, BuiltinFilterFn> {
    let mut m = HashMap::new();
    // git
    m.insert("git status", git::filter_git_status as BuiltinFilterFn);
    m.insert("git diff", git::filter_git_diff as BuiltinFilterFn);
    m.insert("git log", git::filter_git_log as BuiltinFilterFn);
    m.insert("git push", git::filter_git_push as BuiltinFilterFn);
    // cargo
    m.insert("cargo test", cargo::filter_cargo_test as BuiltinFilterFn);
    m.insert("cargo build", cargo::filter_cargo_build as BuiltinFilterFn);
    m.insert(
        "cargo clippy",
        cargo::filter_cargo_clippy as BuiltinFilterFn,
    );
    // npm
    m.insert("npm test", npm::filter_npm_test as BuiltinFilterFn);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_expected_commands() {
        let reg = registry();
        assert!(reg.contains_key("git status"));
        assert!(reg.contains_key("git diff"));
        assert!(reg.contains_key("git log"));
        assert!(reg.contains_key("git push"));
        assert!(reg.contains_key("cargo test"));
        assert!(reg.contains_key("cargo build"));
        assert!(reg.contains_key("cargo clippy"));
        assert!(reg.contains_key("npm test"));
        assert_eq!(reg.len(), 8);
    }

    #[test]
    fn registry_functions_are_callable() {
        let reg = registry();
        let git_status_fn = reg.get("git status").unwrap();
        let result = git_status_fn("On branch main\nnothing to commit", 0);
        assert!(!result.is_empty());
    }
}
