use std::collections::HashMap;
use std::sync::LazyLock;

pub mod cargo;
pub mod docker;
pub mod firebase;
pub mod fs;
pub mod gh;
pub mod git;
pub mod git_extra;
pub mod golang;
pub mod jsbuild;
pub mod npm;
pub mod php;
pub mod python;
pub mod supabase;
pub mod testrunners;
pub mod util;

/// A builtin filter function: takes raw output + exit code, returns compressed output.
pub type BuiltinFilterFn = fn(output: &str, exit_code: i32) -> String;

/// Lazily-initialized global registry of all builtin filters.
static REGISTRY: LazyLock<HashMap<&'static str, BuiltinFilterFn>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    git::register(&mut m);
    git_extra::register(&mut m);
    cargo::register(&mut m);
    npm::register(&mut m);
    gh::register(&mut m);
    fs::register(&mut m);
    testrunners::register(&mut m);
    jsbuild::register(&mut m);
    docker::register(&mut m);
    firebase::register(&mut m);
    python::register(&mut m);
    golang::register(&mut m);
    php::register(&mut m);
    supabase::register(&mut m);
    util::register(&mut m);
    m
});

/// Get the global builtin filter registry.
pub fn registry() -> &'static HashMap<&'static str, BuiltinFilterFn> {
    &REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_expected_commands() {
        let reg = registry();
        // Original 8
        assert!(reg.contains_key("git status"));
        assert!(reg.contains_key("git diff"));
        assert!(reg.contains_key("git log"));
        assert!(reg.contains_key("git push"));
        assert!(reg.contains_key("cargo test"));
        assert!(reg.contains_key("cargo build"));
        assert!(reg.contains_key("cargo clippy"));
        assert!(reg.contains_key("npm test"));
        // New handlers
        assert!(reg.contains_key("git show"));
        assert!(reg.contains_key("gh pr list"));
        assert!(reg.contains_key("docker ps"));
        assert!(reg.contains_key("pytest"));
        assert!(reg.contains_key("tsc"));
        assert!(reg.contains_key("vue-tsc"));
        assert!(reg.contains_key("vite build"));
        assert!(reg.contains_key("vite"));
        assert!(reg.contains_key("go build"));
        assert!(reg.contains_key("ls"));
        assert!(reg.contains_key("curl"));
        assert!(reg.contains_key("supabase status"));
    }

    #[test]
    fn registry_functions_are_callable() {
        let reg = registry();
        let git_status_fn = reg.get("git status").unwrap();
        let result = git_status_fn("On branch main\nnothing to commit", 0);
        assert!(!result.is_empty());
    }

    #[test]
    fn registry_has_minimum_handler_count() {
        let reg = registry();
        // We should have at least 40 handlers
        assert!(
            reg.len() >= 40,
            "Expected at least 40 handlers, got {}",
            reg.len()
        );
    }
}
