use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register JS/TS build tool handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("tsc", filter_tsc as BuiltinFilterFn);
    m.insert("eslint", filter_eslint as BuiltinFilterFn);
    m.insert("prettier", filter_prettier as BuiltinFilterFn);
    m.insert("next build", filter_next_build as BuiltinFilterFn);
}

/// Filter tsc output: on success "No type errors." On failure, keep error lines and count them.
pub fn filter_tsc(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        return "No type errors.".to_string();
    }

    let error_re = Regex::new(r"^.+\(\d+,\d+\):\s+error\s+TS\d+:").unwrap();
    let mut errors: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if error_re.is_match(trimmed) {
            errors.push(trimmed.to_string());
        }
    }

    if errors.is_empty() {
        return format!("Type check failed (exit code {exit_code}).");
    }

    let count = errors.len();
    errors.push(String::new());
    errors.push(format!("{count} error(s) found."));
    errors.join("\n")
}

/// Filter eslint output: keep file paths + error/warning lines, show summary.
pub fn filter_eslint(output: &str, exit_code: i32) -> String {
    if exit_code == 0 && output.trim().is_empty() {
        return "No lint errors.".to_string();
    }

    let file_re = Regex::new(r"^(/|[A-Z]:\\|\./|\.\.\/)").unwrap();
    let diag_re = Regex::new(r"^\s+\d+:\d+\s+(error|warning)\s+").unwrap();
    let summary_re = Regex::new(r"^\u{2716}\s+\d+\s+problem").unwrap();
    let summary_alt_re = Regex::new(r"^\d+\s+problem").unwrap();

    let mut lines: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // File path headers
        if file_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }

        // Error/warning diagnostic lines (e.g. "  3:10  error  ...")
        if diag_re.is_match(line) {
            lines.push(trimmed.to_string());
            continue;
        }

        // Summary line (e.g. "✖ 5 problems (3 errors, 2 warnings)")
        if summary_re.is_match(trimmed) || summary_alt_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }
    }

    if lines.is_empty() {
        if exit_code == 0 {
            "No lint errors.".to_string()
        } else {
            format!("Lint failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter prettier output: on success "All files formatted." On failure, list unformatted files.
pub fn filter_prettier(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        return "All files formatted.".to_string();
    }

    // prettier --check lists files that need formatting, one per line
    let file_re = Regex::new(r"\.\w+$").unwrap();
    let mut unformatted: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip diff output lines (leading +, -, @@)
        if trimmed.starts_with('+')
            || trimmed.starts_with('-')
            || trimmed.starts_with("@@")
            || trimmed.starts_with("diff ")
            || trimmed.starts_with("index ")
        {
            continue;
        }

        // Skip "Checking ..." progress lines
        if trimmed.starts_with("Checking ") {
            continue;
        }

        // Lines that look like file paths (contain a file extension)
        if file_re.is_match(trimmed) && !trimmed.contains(' ') {
            unformatted.push(trimmed.to_string());
            continue;
        }

        // "[warn] filename" format from prettier --check
        if trimmed.starts_with("[warn]") {
            let path = trimmed.trim_start_matches("[warn]").trim();
            if !path.is_empty() && !path.contains("Code style issues") {
                unformatted.push(path.to_string());
            }
            continue;
        }
    }

    if unformatted.is_empty() {
        return format!("Formatting check failed (exit code {exit_code}).");
    }

    let count = unformatted.len();
    let mut result = vec!["Files needing formatting:".to_string()];
    for f in &unformatted {
        result.push(format!("  {f}"));
    }
    result.push(format!("{count} file(s) need formatting."));
    result.join("\n")
}

/// Filter next build output: on success keep route table + bundle size summary.
/// On failure keep error messages.
pub fn filter_next_build(output: &str, exit_code: i32) -> String {
    if exit_code != 0 {
        return filter_next_build_failure(output, exit_code);
    }

    let route_re = Regex::new(r"^[├└┌│○●λƒ\s]*(/\S*|─)").unwrap();
    let size_re = Regex::new(r"^\s*(Route|Size|First Load|○|●|ƒ|λ|\+\s+First)").unwrap();
    let summary_re =
        Regex::new(r"(?i)(first load js|total|shared by all|chunks|bundle size)").unwrap();

    let mut lines: Vec<String> = Vec::new();
    let mut in_route_table = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip progress lines
        if trimmed.starts_with("Creating an optimized")
            || trimmed.starts_with("Compiling")
            || trimmed.starts_with("Collecting page data")
            || trimmed.starts_with("Generating static pages")
            || trimmed.starts_with("Finalizing page optimization")
            || trimmed.starts_with("info")
            || trimmed.is_empty()
        {
            if in_route_table && trimmed.is_empty() {
                // End of route table block — keep a separator
                lines.push(String::new());
                in_route_table = false;
            }
            continue;
        }

        // Route table header
        if trimmed.starts_with("Route") || size_re.is_match(trimmed) {
            in_route_table = true;
            lines.push(trimmed.to_string());
            continue;
        }

        // Route table entries
        if in_route_table && (route_re.is_match(trimmed) || trimmed.starts_with("+ First")) {
            lines.push(trimmed.to_string());
            continue;
        }

        // Bundle size summary
        if summary_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }

        // Legend lines (○ static, ● SSG, ƒ dynamic, etc.)
        if trimmed.starts_with('○')
            || trimmed.starts_with('●')
            || trimmed.starts_with('ƒ')
            || trimmed.starts_with('λ')
        {
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        "Build completed successfully.".to_string()
    } else {
        // Remove trailing empty lines
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
}

fn filter_next_build_failure(output: &str, exit_code: i32) -> String {
    let mut lines: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Error:")
            || trimmed.starts_with("error")
            || trimmed.starts_with("Error")
            || trimmed.starts_with("Type error:")
            || trimmed.starts_with("Build error")
            || trimmed.starts_with("Failed to compile")
            || trimmed.starts_with("Module not found")
            || trimmed.contains("Cannot find module")
        {
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        format!("Build failed (exit code {exit_code}).")
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- tsc --

    #[test]
    fn tsc_success() {
        let result = filter_tsc("", 0);
        assert_eq!(result, "No type errors.");
    }

    #[test]
    fn tsc_with_errors() {
        let input = "\
src/app.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'.
src/app.ts(15,3): error TS2345: Argument of type 'boolean' is not assignable.
src/utils.ts(3,1): error TS1005: ';' expected.";
        let result = filter_tsc(input, 2);
        assert!(result.contains("src/app.ts(10,5): error TS2322"));
        assert!(result.contains("src/utils.ts(3,1): error TS1005"));
        assert!(result.contains("3 error(s) found."));
    }

    #[test]
    fn tsc_failure_no_parseable_errors() {
        let input = "Unknown compiler error\nSomething went wrong";
        let result = filter_tsc(input, 1);
        assert_eq!(result, "Type check failed (exit code 1).");
    }

    #[test]
    fn tsc_drops_non_error_lines() {
        let input = "\
Version 5.3.2
src/index.ts(1,1): error TS2304: Cannot find name 'foo'.
Found 1 error.";
        let result = filter_tsc(input, 2);
        assert!(result.contains("error TS2304"));
        assert!(!result.contains("Version"));
        assert!(!result.contains("Found 1 error")); // we provide our own count
        assert!(result.contains("1 error(s) found."));
    }

    // -- eslint --

    #[test]
    fn eslint_clean() {
        let result = filter_eslint("", 0);
        assert_eq!(result, "No lint errors.");
    }

    #[test]
    fn eslint_with_errors() {
        let input = "\
/home/user/project/src/app.ts
  3:10  error  Unexpected console statement  no-console
  7:1   warning  Missing return type          @typescript-eslint/explicit-function-return-type

/home/user/project/src/utils.ts
  12:5  error  'x' is assigned but never used  no-unused-vars

\u{2716} 3 problems (2 errors, 1 warning)";
        let result = filter_eslint(input, 1);
        assert!(result.contains("/home/user/project/src/app.ts"));
        assert!(result.contains("3:10  error  Unexpected console statement"));
        assert!(result.contains("7:1   warning  Missing return type"));
        assert!(result.contains("/home/user/project/src/utils.ts"));
        assert!(result.contains("3 problems (2 errors, 1 warning)"));
    }

    #[test]
    fn eslint_drops_source_context() {
        let input = "\
./src/app.ts
  3:10  error  Unexpected console statement  no-console

    console.log('hello');
    ^^^^^^^^^

\u{2716} 1 problem (1 error, 0 warnings)";
        let result = filter_eslint(input, 1);
        assert!(result.contains("3:10  error"));
        assert!(!result.contains("console.log"));
        assert!(!result.contains("^^^^^^^^^"));
    }

    #[test]
    fn eslint_failure_no_parseable_output() {
        let input = "Oops, something went wrong!";
        let result = filter_eslint(input, 2);
        assert_eq!(result, "Lint failed (exit code 2).");
    }

    // -- prettier --

    #[test]
    fn prettier_success() {
        let result = filter_prettier("", 0);
        assert_eq!(result, "All files formatted.");
    }

    #[test]
    fn prettier_check_failures() {
        let input = "\
[warn] src/app.ts
[warn] src/utils.ts
[warn] src/components/Button.tsx
[warn] Code style issues found. Run Prettier to fix.";
        let result = filter_prettier(input, 1);
        assert!(result.contains("Files needing formatting:"));
        assert!(result.contains("src/app.ts"));
        assert!(result.contains("src/utils.ts"));
        assert!(result.contains("src/components/Button.tsx"));
        assert!(result.contains("3 file(s) need formatting."));
        assert!(!result.contains("Code style issues"));
    }

    #[test]
    fn prettier_drops_diff_details() {
        let input = "\
src/app.ts
diff --git a/src/app.ts b/src/app.ts
index abc123..def456 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,3 +1,3 @@
-const x = 1
+const x = 1;";
        let result = filter_prettier(input, 1);
        assert!(result.contains("src/app.ts"));
        assert!(!result.contains("diff --git"));
        assert!(!result.contains("index abc123"));
        assert!(!result.contains("const x = 1"));
    }

    #[test]
    fn prettier_failure_no_files() {
        let input = "Some unknown error occurred";
        let result = filter_prettier(input, 1);
        assert_eq!(result, "Formatting check failed (exit code 1).");
    }

    // -- next build --

    #[test]
    fn next_build_success_with_routes() {
        let input = "\
info  - Creating an optimized production build
info  - Compiled successfully
info  - Collecting page data
info  - Generating static pages (4/4)
info  - Finalizing page optimization

Route (app)                              Size     First Load JS
\u{250c} ○ /                                    5.23 kB        89.2 kB
\u{251c} ○ /about                               2.11 kB        86.1 kB
\u{2514} ƒ /api/hello                            0 B            84.0 kB
+ First Load JS shared by all            84.0 kB

○  (Static)  prerendered as static content
ƒ  (Dynamic) server-rendered on demand";
        let result = filter_next_build(input, 0);
        assert!(result.contains("Route (app)"));
        assert!(result.contains("○ /"));
        assert!(result.contains("○ /about"));
        assert!(result.contains("ƒ /api/hello"));
        assert!(result.contains("+ First Load JS shared by all"));
        assert!(result.contains("○  (Static)"));
        assert!(!result.contains("Creating an optimized"));
        assert!(!result.contains("Compiled successfully"));
        assert!(!result.contains("Collecting page data"));
    }

    #[test]
    fn next_build_failure() {
        let input = "\
info  - Creating an optimized production build
Failed to compile.

Type error: Cannot find name 'foo'.

Error: Build failed because of webpack errors";
        let result = filter_next_build(input, 1);
        assert!(result.contains("Failed to compile"));
        assert!(result.contains("Type error: Cannot find name 'foo'"));
        assert!(result.contains("Error: Build failed"));
        assert!(!result.contains("Creating an optimized"));
    }

    #[test]
    fn next_build_success_no_route_table() {
        let input = "\
info  - Creating an optimized production build
info  - Compiled successfully";
        let result = filter_next_build(input, 0);
        assert_eq!(result, "Build completed successfully.");
    }

    #[test]
    fn next_build_failure_module_not_found() {
        let input = "\
Compiling ...
Module not found: Can't resolve 'lodash'
Error: Module not found";
        let result = filter_next_build(input, 1);
        assert!(result.contains("Module not found: Can't resolve 'lodash'"));
    }
}
