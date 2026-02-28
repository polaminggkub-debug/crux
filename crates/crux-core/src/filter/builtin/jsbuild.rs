use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register JS/TS build tool handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("tsc", filter_tsc as BuiltinFilterFn);
    m.insert("vue-tsc", filter_tsc as BuiltinFilterFn);
    m.insert("eslint", filter_eslint as BuiltinFilterFn);
    m.insert("prettier", filter_prettier as BuiltinFilterFn);
    m.insert("next build", filter_next_build as BuiltinFilterFn);
    m.insert("vite build", filter_vite_build as BuiltinFilterFn);
    m.insert("vite", filter_vite_build as BuiltinFilterFn);
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

/// Filter vite build output: on success keep summary + top 5 largest JS assets.
/// On failure keep error and warning lines.
pub fn filter_vite_build(output: &str, exit_code: i32) -> String {
    if exit_code != 0 {
        return filter_vite_build_failure(output, exit_code);
    }

    let transformed_re = Regex::new(r"✓\s+\d+\s+modules?\s+transformed").unwrap();
    let built_re = Regex::new(r"✓\s+built\s+in\s+").unwrap();
    let warning_re = Regex::new(r"^\(\!\)").unwrap();
    let asset_re =
        Regex::new(r"^(dist/\S+\.js)\s+([\d.]+)\s+(kB|B)\s+│\s+gzip:\s+([\d.]+)\s+(kB|B)").unwrap();

    let mut summary_lines: Vec<String> = Vec::new();
    let mut warning_lines: Vec<String> = Vec::new();
    let mut js_assets: Vec<(String, f64)> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if transformed_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        if built_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        if warning_re.is_match(trimmed) {
            warning_lines.push(trimmed.to_string());
            continue;
        }

        if let Some(caps) = asset_re.captures(trimmed) {
            let path = caps[1].to_string();
            let size: f64 = caps[2].parse().unwrap_or(0.0);
            let unit = &caps[3];
            let size_kb = if unit == "B" { size / 1024.0 } else { size };
            js_assets.push((path, size_kb));
        }
    }

    // Sort by size descending, take top 5
    js_assets.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_assets: Vec<String> = js_assets
        .iter()
        .take(5)
        .map(|(path, size_kb)| format!("  {path}  {size_kb:.2} kB"))
        .collect();

    let mut result: Vec<String> = Vec::new();
    result.extend(summary_lines);
    if !top_assets.is_empty() {
        result.push(format!("Top {} JS assets:", top_assets.len()));
        result.extend(top_assets);
    }
    result.extend(warning_lines);

    if result.is_empty() {
        "Build completed successfully.".to_string()
    } else {
        result.join("\n")
    }
}

fn filter_vite_build_failure(output: &str, exit_code: i32) -> String {
    let warning_re = Regex::new(r"^\(\!\)").unwrap();
    let mut lines: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("error") || trimmed.contains("Error") || warning_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        format!("Build failed (exit code {exit_code}).")
    } else {
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

    // -- vite build --

    #[test]
    fn vite_build_success_keeps_summary_and_top_assets() {
        let input = "\
vite v6.0.11 building for production...
transforming (1010) ...
✓ 1010 modules transformed.
dist/assets/PositionsPage-tn0RQdqM.css    0.00 kB │ gzip:   0.02 kB
dist/assets/LoginPage-vxzrQSRd.css        0.55 kB │ gzip:   0.31 kB
dist/assets/vendor-abc123.js             180.00 kB │ gzip:  50.00 kB
dist/assets/index-BFZsO9Dd.js           380.94 kB │ gzip:  90.77 kB
dist/assets/utils-xyz789.js               25.30 kB │ gzip:   8.10 kB
dist/assets/chart-def456.js               90.50 kB │ gzip:  30.20 kB
dist/assets/tiny-ghi012.js                1.20 kB │ gzip:   0.50 kB
dist/assets/auth-jkl345.js               45.60 kB │ gzip:  15.30 kB
dist/assets/router-mno678.js             12.00 kB │ gzip:   4.00 kB
✓ built in 12.22s";
        let result = filter_vite_build(input, 0);
        // Summary lines kept
        assert!(result.contains("✓ 1010 modules transformed."));
        assert!(result.contains("✓ built in 12.22s"));
        // Top 5 largest JS assets kept (index, vendor, chart, auth, utils)
        assert!(result.contains("index-BFZsO9Dd.js"));
        assert!(result.contains("vendor-abc123.js"));
        assert!(result.contains("chart-def456.js"));
        assert!(result.contains("auth-jkl345.js"));
        assert!(result.contains("utils-xyz789.js"));
        // Smaller ones excluded
        assert!(!result.contains("tiny-ghi012.js"));
        assert!(!result.contains("router-mno678.js"));
        // CSS excluded (not JS assets)
        assert!(!result.contains("PositionsPage"));
        assert!(!result.contains("LoginPage"));
        // Progress lines excluded
        assert!(!result.contains("building for production"));
        assert!(!result.contains("transforming (1010)"));
    }

    #[test]
    fn vite_build_success_with_warnings() {
        let input = "\
✓ 500 modules transformed.
dist/assets/index-abc.js  380.94 kB │ gzip:  90.77 kB
✓ built in 5.00s

(!) Some chunks are larger than 500 kB after minification.";
        let result = filter_vite_build(input, 0);
        assert!(result.contains("✓ 500 modules transformed."));
        assert!(result.contains("✓ built in 5.00s"));
        assert!(result.contains("(!) Some chunks are larger than 500 kB"));
        assert!(result.contains("index-abc.js"));
    }

    #[test]
    fn vite_build_success_fewer_than_5_assets() {
        let input = "\
✓ 100 modules transformed.
dist/assets/index-abc.js    50.00 kB │ gzip:  15.00 kB
dist/assets/vendor-def.js  120.00 kB │ gzip:  40.00 kB
✓ built in 2.00s";
        let result = filter_vite_build(input, 0);
        assert!(result.contains("Top 2 JS assets:"));
        assert!(result.contains("vendor-def.js"));
        assert!(result.contains("index-abc.js"));
    }

    #[test]
    fn vite_build_failure_keeps_errors() {
        let input = "\
vite v6.0.11 building for production...
transforming (500) ...
[vite]: Rollup failed to resolve import \"missing-pkg\"
error during build:
Error: Could not resolve entry module \"src/main.ts\"";
        let result = filter_vite_build(input, 1);
        assert!(result.contains("error during build:"));
        assert!(result.contains("Error: Could not resolve entry module"));
        assert!(!result.contains("building for production"));
    }

    #[test]
    fn vite_build_failure_no_parseable_errors() {
        let input = "Something unexpected happened\nNo useful info here";
        let result = filter_vite_build(input, 1);
        assert_eq!(result, "Build failed (exit code 1).");
    }

    #[test]
    fn vite_build_failure_with_warnings() {
        let input = "\
(!) Could not resolve dependency
error during build:
Some other output";
        let result = filter_vite_build(input, 1);
        assert!(result.contains("(!) Could not resolve dependency"));
        assert!(result.contains("error during build:"));
    }

    #[test]
    fn vite_build_success_no_output() {
        let result = filter_vite_build("", 0);
        assert_eq!(result, "Build completed successfully.");
    }
}
