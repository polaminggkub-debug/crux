//! Verify embedded stdlib filter test suites.
//!
//! Each `_test/` directory next to a `.toml` filter in the embedded stdlib
//! should contain pairs of files:
//!   - `input.txt` / `expected.txt` (single test case)
//!   - `<name>.input` / `<name>.expected` (named test cases)

use include_dir::{include_dir, Dir};

use crate::config::FilterConfig;
use crate::filter::apply_filter;

static STDLIB_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/filters");

/// Result of a single test case.
#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

/// Result of verifying all embedded stdlib test suites.
#[derive(Debug)]
pub struct VerifyResult {
    pub results: Vec<TestResult>,
}

impl VerifyResult {
    pub fn total(&self) -> usize {
        self.results.len()
    }

    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }
}

/// Verify all embedded stdlib filter test suites.
pub fn verify_embedded_stdlib() -> VerifyResult {
    let mut results = Vec::new();
    verify_embedded_dir(&STDLIB_DIR, &mut results);
    VerifyResult { results }
}

fn verify_embedded_dir(dir: &Dir<'_>, results: &mut Vec<TestResult>) {
    // Look for _test directories
    for subdir in dir.dirs() {
        let dir_name = subdir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if dir_name.ends_with("_test") {
            let base_name = dir_name.strip_suffix("_test").unwrap_or(dir_name);
            // Find the corresponding .toml file
            let toml_filename = format!("{base_name}.toml");
            if let Some(toml_file) = dir.get_file(dir.path().join(&toml_filename)) {
                if let Some(toml_contents) = toml_file.contents_utf8() {
                    if let Ok(config) = toml::from_str::<FilterConfig>(toml_contents) {
                        run_embedded_test_suite(&config, subdir, results);
                    }
                }
            }
        } else {
            // Recurse into non-test subdirectories
            verify_embedded_dir(subdir, results);
        }
    }
}

fn run_embedded_test_suite(
    config: &FilterConfig,
    test_dir: &Dir<'_>,
    results: &mut Vec<TestResult>,
) {
    // Check for input.txt / expected.txt pair (single test case)
    let input_txt = test_dir
        .get_file(test_dir.path().join("input.txt"))
        .and_then(|f| f.contents_utf8());
    let expected_txt = test_dir
        .get_file(test_dir.path().join("expected.txt"))
        .and_then(|f| f.contents_utf8());

    if let (Some(input), Some(expected)) = (input_txt, expected_txt) {
        let actual = apply_filter(config, input, 0);
        let passed = actual.trim() == expected.trim();
        results.push(TestResult {
            name: format!("{}::default", config.command),
            passed,
            expected: expected.to_string(),
            actual,
        });
    }

    // Check for <name>.input / <name>.expected pairs
    for file in test_dir.files() {
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) == Some("input") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let expected_path = test_dir.path().join(format!("{stem}.expected"));
            if let Some(expected_file) = test_dir.get_file(&expected_path) {
                if let (Some(input), Some(expected)) =
                    (file.contents_utf8(), expected_file.contents_utf8())
                {
                    let actual = apply_filter(config, input, 0);
                    let passed = actual.trim() == expected.trim();
                    results.push(TestResult {
                        name: format!("{}::{stem}", config.command),
                        passed,
                        expected: expected.to_string(),
                        actual,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_stdlib_tests_pass() {
        let result = verify_embedded_stdlib();
        assert!(
            result.total() > 0,
            "Expected at least one embedded test case"
        );
        for tr in &result.results {
            assert!(
                tr.passed,
                "Test '{}' failed.\nExpected:\n{}\nActual:\n{}",
                tr.name,
                tr.expected.trim(),
                tr.actual.trim()
            );
        }
    }
}
