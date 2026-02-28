use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register PHP / Laravel / Composer handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    // Test runners
    m.insert("phpunit", filter_phpunit as BuiltinFilterFn);
    m.insert("pest", filter_pest as BuiltinFilterFn);
    m.insert("php artisan test", filter_artisan_test as BuiltinFilterFn);

    // Laravel Artisan
    m.insert("php artisan migrate", filter_artisan_migrate as BuiltinFilterFn);
    m.insert(
        "php artisan migrate:fresh",
        filter_artisan_migrate as BuiltinFilterFn,
    );
    m.insert(
        "php artisan migrate:rollback",
        filter_artisan_migrate as BuiltinFilterFn,
    );
    m.insert(
        "php artisan migrate:status",
        filter_artisan_migrate_status as BuiltinFilterFn,
    );
    m.insert(
        "php artisan route:list",
        filter_artisan_route_list as BuiltinFilterFn,
    );
    m.insert("php artisan", filter_artisan_generic as BuiltinFilterFn);

    // Composer
    m.insert("composer install", filter_composer_install as BuiltinFilterFn);
    m.insert("composer update", filter_composer_install as BuiltinFilterFn);
    m.insert("composer require", filter_composer_require as BuiltinFilterFn);
}

/// Filter PHPUnit output: keep summary line, on failure keep failure names and assertion messages.
pub fn filter_phpunit(output: &str, exit_code: i32) -> String {
    let summary_re =
        Regex::new(r"(?i)^(OK \(|Tests:|FAILURES!|ERRORS!|There was|Time:)").unwrap();
    let result_re =
        Regex::new(r"(?i)^\s*(OK|FAILURES!|ERRORS!)\s*(\(|$)").unwrap();
    let test_count_re =
        Regex::new(r"(?i)^(Tests:\s*\d+|OK \(\d+ test)").unwrap();
    let fail_header_re = Regex::new(r"^\d+\)\s+\S+").unwrap();
    let assertion_re =
        Regex::new(r"(?i)(Failed assert|Expected|Actual|---\s+Expected|\+\+\+\s+Actual|PHPUnit)")
            .unwrap();
    let separator_re = Regex::new(r"^-{3,}$|^\.{3,}$").unwrap();

    let mut summary_lines = Vec::new();
    let mut failure_lines = Vec::new();
    let mut in_failure = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || separator_re.is_match(trimmed) {
            continue;
        }

        // Progress dots (........F..E..)
        if !trimmed.is_empty()
            && trimmed.chars().all(|c| c == '.' || c == 'F' || c == 'E' || c == 'S' || c == 'R' || c == 'I' || c == 'W' || c == ' ')
            && trimmed.len() > 3
        {
            continue;
        }

        // Summary/result lines
        if result_re.is_match(trimmed) || test_count_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            in_failure = false;
            continue;
        }

        if summary_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // Failure header: "1) App\Tests\UserTest::testLogin"
        if fail_header_re.is_match(trimmed) {
            in_failure = true;
            failure_lines.push(trimmed.to_string());
            continue;
        }

        // Inside failure block: capture assertion details
        if in_failure && assertion_re.is_match(trimmed) {
            failure_lines.push(format!("  {trimmed}"));
        }
    }

    let mut parts = Vec::new();

    if exit_code != 0 && !failure_lines.is_empty() {
        parts.push("Failures:".to_string());
        for line in &failure_lines {
            parts.push(line.clone());
        }
        parts.push(String::new());
    }

    if !summary_lines.is_empty() {
        for line in &summary_lines {
            parts.push(line.clone());
        }
    } else if exit_code == 0 {
        parts.push("All tests passed.".to_string());
    } else {
        parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    parts.join("\n")
}

/// Filter Pest output: similar to PHPUnit but with Pest-specific formatting.
/// Pest uses ✓/✗ marks, "Tests: N passed, N failed" summary.
pub fn filter_pest(output: &str, exit_code: i32) -> String {
    let summary_re = Regex::new(r"(?i)^\s*Tests:\s+\d+").unwrap();
    let pass_re = Regex::new(r"^\s*✓\s+").unwrap();
    let fail_re = Regex::new(r"^\s*(✗|×|FAIL)\s+").unwrap();
    let error_detail_re =
        Regex::new(r"(?i)(Expected|Actual|Failed assert|toBe|toEqual|assert|Exception)")
            .unwrap();
    let duration_re = Regex::new(r"^\s*Duration:?\s+[\d.]+").unwrap();

    let mut summary_lines = Vec::new();
    let mut failure_lines = Vec::new();
    let mut in_failure = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            in_failure = false;
            continue;
        }

        // Summary line: "Tests: 8 passed, 1 failed"
        if summary_re.is_match(trimmed) || duration_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            in_failure = false;
            continue;
        }

        // Passing tests — skip to save tokens
        if pass_re.is_match(trimmed) {
            in_failure = false;
            continue;
        }

        // Failing test header
        if fail_re.is_match(trimmed) {
            failure_lines.push(trimmed.to_string());
            in_failure = true;
            continue;
        }

        // Error details inside failure
        if in_failure && error_detail_re.is_match(trimmed) {
            failure_lines.push(format!("  {trimmed}"));
        }
    }

    let mut parts = Vec::new();

    if exit_code != 0 && !failure_lines.is_empty() {
        parts.push("Failures:".to_string());
        for line in &failure_lines {
            parts.push(line.clone());
        }
        parts.push(String::new());
    }

    if !summary_lines.is_empty() {
        for line in &summary_lines {
            parts.push(line.clone());
        }
    } else if exit_code == 0 {
        parts.push("All tests passed.".to_string());
    } else {
        parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    parts.join("\n")
}

/// Filter `php artisan test` — wraps PHPUnit/Pest, same output format.
pub fn filter_artisan_test(output: &str, exit_code: i32) -> String {
    // artisan test wraps PHPUnit or Pest — try Pest patterns first, fall back to PHPUnit
    let has_pest = output.contains("✓ ") || output.contains("✗ ");
    if has_pest {
        filter_pest(output, exit_code)
    } else {
        filter_phpunit(output, exit_code)
    }
}

/// Filter `php artisan migrate` output: keep migration names and status.
pub fn filter_artisan_migrate(output: &str, exit_code: i32) -> String {
    let migration_re =
        Regex::new(r"(?i)^\s*(Migrating|Migrated|Rolling back|Rolled back|INFO|WARN)\s")
            .unwrap();
    let table_re = Regex::new(r"(?i)(dropping|creating|dropped|created)\s+\S+\s+table")
        .unwrap();
    let done_re = Regex::new(r"(?i)(nothing to migrate|migration complete|done)").unwrap();
    let error_re = Regex::new(r"(?i)(error|exception|failed|SQLSTATE)").unwrap();

    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if migration_re.is_match(trimmed)
            || table_re.is_match(trimmed)
            || done_re.is_match(trimmed)
        {
            lines.push(trimmed.to_string());
            continue;
        }

        if exit_code != 0 && error_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        if exit_code == 0 {
            "Migration completed.".to_string()
        } else {
            format!("Migration failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter `php artisan migrate:status` — keep the table but remove decorative borders.
pub fn filter_artisan_migrate_status(output: &str, exit_code: i32) -> String {
    let border_re = Regex::new(r"^[\s|+\-]+$").unwrap();
    let header_re = Regex::new(r"(?i)(migration name|batch|ran\?)").unwrap();
    let status_re = Regex::new(r"(?i)(yes|no|ran|pending)").unwrap();

    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || border_re.is_match(trimmed) {
            continue;
        }

        if header_re.is_match(trimmed) || status_re.is_match(trimmed) {
            // Clean pipe separators for compactness
            let clean = trimmed
                .replace("| ", "")
                .replace(" |", "")
                .replace("|", " ");
            lines.push(clean.trim().to_string());
        }
    }

    if lines.is_empty() {
        if exit_code == 0 {
            "No migrations found.".to_string()
        } else {
            format!("migrate:status failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter `php artisan route:list` — keep routes, remove decorative borders,
/// compress spacing.
pub fn filter_artisan_route_list(output: &str, exit_code: i32) -> String {
    let border_re = Regex::new(r"^[\s+\-]+$").unwrap();
    let method_re =
        Regex::new(r"(?i)(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|ANY)").unwrap();
    let header_re = Regex::new(r"(?i)(method|uri|name|action|middleware)").unwrap();
    let whitespace_re = Regex::new(r"\s{2,}").unwrap();

    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || border_re.is_match(trimmed) {
            continue;
        }

        if header_re.is_match(trimmed) || method_re.is_match(trimmed) {
            // Clean pipe separators and compress whitespace
            let clean = trimmed
                .replace("| ", "")
                .replace(" |", "")
                .replace("|", " ");
            let compressed = whitespace_re.replace_all(&clean, "  ").trim().to_string();
            lines.push(compressed);
        }
    }

    if lines.is_empty() {
        if exit_code == 0 {
            "No routes found.".to_string()
        } else {
            format!("route:list failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter generic `php artisan` commands: keep INFO/WARN/ERROR lines and key output.
pub fn filter_artisan_generic(output: &str, exit_code: i32) -> String {
    let info_re = Regex::new(r"(?i)^\s*(INFO|WARN|ERROR|SUCCESS|DONE)\s").unwrap();
    let error_re = Regex::new(r"(?i)(error|exception|failed|SQLSTATE)").unwrap();
    let result_re =
        Regex::new(r"(?i)(created|generated|published|cached|cleared|compiled|seeded|optimized)")
            .unwrap();

    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if info_re.is_match(trimmed) || result_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }

        if exit_code != 0 && error_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        if exit_code == 0 {
            "Command completed.".to_string()
        } else {
            format!("Command failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter `composer install/update`: keep summary, warnings, and errors.
pub fn filter_composer_install(output: &str, exit_code: i32) -> String {
    let summary_re = Regex::new(
        r"(?i)(installing|updating|nothing to install|lock file|package operations|Generating|No security)",
    )
    .unwrap();
    let package_op_re = Regex::new(r"(?i)^\s*-\s+(Installing|Updating|Removing)\s+").unwrap();
    let warning_re = Regex::new(r"(?i)(warning|deprecated)").unwrap();
    let error_re = Regex::new(r"(?i)(error|failed|could not|problem \d+)").unwrap();
    let autoload_re = Regex::new(r"(?i)(autoload|dumping|generated)").unwrap();

    let mut lines = Vec::new();
    let mut package_count = 0;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Count package operations instead of listing each one
        if package_op_re.is_match(trimmed) {
            package_count += 1;
            continue;
        }

        if summary_re.is_match(trimmed) || autoload_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }

        if warning_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }

        if exit_code != 0 && error_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
        }
    }

    if package_count > 0 {
        lines.insert(0, format!("{package_count} package operations."));
    }

    if lines.is_empty() {
        if exit_code == 0 {
            "Installed successfully.".to_string()
        } else {
            format!("Install failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter `composer require`: keep what was added and any errors.
pub fn filter_composer_require(output: &str, exit_code: i32) -> String {
    // Reuse composer install filter — same output patterns
    filter_composer_install(output, exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- PHPUnit --

    #[test]
    fn phpunit_pass() {
        let input = "\
PHPUnit 10.5.2 by Sebastian Bergmann and contributors.

Runtime:       PHP 8.3.1

...............                                                   15 / 15 (100%)

Time: 00:00.234, Memory: 12.00 MB

OK (15 tests, 30 assertions)";

        let result = filter_phpunit(input, 0);
        assert!(result.contains("OK (15 tests, 30 assertions)"));
        assert!(!result.contains("Sebastian Bergmann"));
        assert!(!result.contains("Runtime"));
        assert!(!result.contains("..............."));
    }

    #[test]
    fn phpunit_failure() {
        let input = "\
PHPUnit 10.5.2 by Sebastian Bergmann and contributors.

Runtime:       PHP 8.3.1

...F..                                                            6 / 6 (100%)

Time: 00:00.456, Memory: 14.00 MB

There was 1 failure:

1) App\\Tests\\UserTest::testCreateUser
Failed asserting that 404 matches expected 200.
Expected :200
Actual   :404

/app/tests/UserTest.php:42

FAILURES!
Tests: 6, Assertions: 10, Failures: 1.";

        let result = filter_phpunit(input, 1);
        assert!(result.contains("Failures:"));
        assert!(result.contains("App\\Tests\\UserTest::testCreateUser"));
        assert!(result.contains("Failed asserting that 404 matches expected 200"));
        assert!(result.contains("FAILURES!"));
        assert!(result.contains("Tests: 6, Assertions: 10, Failures: 1"));
        assert!(!result.contains("Sebastian Bergmann"));
        assert!(!result.contains("Runtime"));
    }

    #[test]
    fn phpunit_empty() {
        let result = filter_phpunit("", 0);
        assert_eq!(result, "All tests passed.");
    }

    // -- Pest --

    #[test]
    fn pest_pass() {
        let input = "\

   PASS  Tests\\Unit\\ExampleTest
  ✓ that true is true

   PASS  Tests\\Feature\\UserTest
  ✓ it can create a user
  ✓ it can list users
  ✓ it validates email

  Tests:    4 passed (8 assertions)
  Duration: 0.52s";

        let result = filter_pest(input, 0);
        assert!(result.contains("Tests:    4 passed (8 assertions)"));
        assert!(result.contains("Duration: 0.52s"));
        assert!(!result.contains("✓ that true is true"));
        assert!(!result.contains("PASS  Tests"));
    }

    #[test]
    fn pest_failure() {
        let input = "\

   PASS  Tests\\Unit\\ExampleTest
  ✓ that true is true

   FAIL  Tests\\Feature\\UserTest
  ✗ it can create a user
  Expected status code 200, but received 500.
  Failed asserting that 500 is identical to 200.

  Tests:    1 failed, 1 passed (3 assertions)
  Duration: 0.89s";

        let result = filter_pest(input, 1);
        assert!(result.contains("Failures:"));
        assert!(result.contains("✗ it can create a user"));
        assert!(result.contains("Failed asserting that 500 is identical to 200"));
        assert!(result.contains("Tests:    1 failed, 1 passed"));
        assert!(!result.contains("✓ that true is true"));
    }

    #[test]
    fn pest_empty() {
        let result = filter_pest("", 0);
        assert_eq!(result, "All tests passed.");
    }

    // -- artisan test --

    #[test]
    fn artisan_test_delegates_to_pest() {
        let input = "\
  ✓ it works
  Tests:    1 passed
  Duration: 0.1s";
        let result = filter_artisan_test(input, 0);
        assert!(result.contains("Tests:    1 passed"));
    }

    #[test]
    fn artisan_test_delegates_to_phpunit() {
        let input = "\
OK (5 tests, 10 assertions)";
        let result = filter_artisan_test(input, 0);
        assert!(result.contains("OK (5 tests, 10 assertions)"));
    }

    // -- artisan migrate --

    #[test]
    fn migrate_success() {
        let input = "\

   INFO  Preparing database.

  Creating migration table ............................................... 13ms DONE

   INFO  Running migrations.

  2024_01_01_000000_create_users_table .................................... 8ms DONE
  2024_01_02_000000_create_posts_table ................................... 12ms DONE
  2024_01_03_000000_create_comments_table ................................. 6ms DONE";

        let result = filter_artisan_migrate(input, 0);
        assert!(result.contains("INFO  Preparing database"));
        assert!(result.contains("INFO  Running migrations"));
        // Should not contain PHP version/runtime boilerplate
        assert!(!result.contains("PHPUnit"));
    }

    #[test]
    fn migrate_nothing() {
        let input = "\

   INFO  Nothing to migrate.";
        let result = filter_artisan_migrate(input, 0);
        assert!(result.contains("INFO  Nothing to migrate"));
    }

    #[test]
    fn migrate_error() {
        let input = "\
SQLSTATE[42S01]: Table already exists
Error: migration failed";
        let result = filter_artisan_migrate(input, 1);
        assert!(result.contains("SQLSTATE"));
        assert!(result.contains("Error: migration failed"));
    }

    // -- composer install --

    #[test]
    fn composer_install_success() {
        let input = "\
Installing dependencies from lock file (including require-dev)
Verifying lock file contents can be installed on current platform.
Package operations: 45 installs, 0 updates, 0 removals
  - Installing psr/http-message (2.0): Extracting archive
  - Installing psr/http-factory (1.0.2): Extracting archive
  - Installing symfony/console (v6.4.1): Extracting archive
  - Installing laravel/framework (v10.38.2): Extracting archive
Generating optimized autoload files
> @php artisan package:discover
No security vulnerability advisories found.";

        let result = filter_composer_install(input, 0);
        assert!(result.contains("package operations"));
        assert!(result.contains("Generating optimized autoload files"));
        assert!(result.contains("No security vulnerability"));
        assert!(!result.contains("Extracting archive"));
    }

    #[test]
    fn composer_install_empty() {
        let input = "Nothing to install, update or remove
Generating optimized autoload files";
        let result = filter_composer_install(input, 0);
        assert!(result.contains("Nothing to install"));
        assert!(result.contains("Generating"));
    }

    #[test]
    fn composer_install_error() {
        let input = "\
Your requirements could not be resolved to an installable set of packages.
  Problem 1
    - laravel/framework requires php ^8.1 -> your PHP version (7.4.0) does not satisfy that requirement.";
        let result = filter_composer_install(input, 2);
        assert!(result.contains("Problem 1"));
    }

    // -- route:list --

    #[test]
    fn route_list_output() {
        let input = "\

  GET|HEAD  / ........................................ home › HomeController@index
  GET|HEAD  api/users ................ users.index › UserController@index
  POST      api/users ................ users.store › UserController@store
  GET|HEAD  api/users/{user} ......... users.show › UserController@show
  PUT|PATCH api/users/{user} ......... users.update › UserController@update
  DELETE    api/users/{user} ......... users.destroy › UserController@destroy

                                                          Showing [6] routes";

        let result = filter_artisan_route_list(input, 0);
        assert!(result.contains("GET"));
        assert!(result.contains("api/users"));
    }

    // -- artisan generic --

    #[test]
    fn artisan_cache_clear() {
        let input = "\

   INFO  Application cache cleared successfully.";
        let result = filter_artisan_generic(input, 0);
        assert!(result.contains("INFO  Application cache cleared successfully"));
    }

    #[test]
    fn artisan_make_model() {
        let input = "\

   INFO  Model [app/Models/Invoice.php] created successfully.";
        let result = filter_artisan_generic(input, 0);
        assert!(result.contains("INFO  Model"));
        assert!(result.contains("created successfully"));
    }

    #[test]
    fn artisan_generic_empty() {
        let result = filter_artisan_generic("", 0);
        assert_eq!(result, "Command completed.");
    }
}
