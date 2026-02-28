use std::collections::HashMap;

use super::BuiltinFilterFn;

/// Register Supabase CLI command handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("supabase status", filter_supabase_status as BuiltinFilterFn);
    m.insert(
        "supabase migration list",
        filter_supabase_migration_list as BuiltinFilterFn,
    );
    m.insert(
        "supabase db diff",
        filter_supabase_db_diff as BuiltinFilterFn,
    );
    m.insert(
        "supabase db reset",
        filter_supabase_db_reset as BuiltinFilterFn,
    );
    m.insert(
        "supabase db push",
        filter_supabase_db_push as BuiltinFilterFn,
    );
    m.insert(
        "supabase start",
        filter_supabase_lifecycle as BuiltinFilterFn,
    );
    m.insert(
        "supabase stop",
        filter_supabase_lifecycle as BuiltinFilterFn,
    );
    m.insert("supabase", filter_supabase_generic as BuiltinFilterFn);
}

/// Secret field names in `supabase status` output that should be masked.
/// Matches both old format ("anon key") and new box format ("Publishable", "Secret Key").
const STATUS_SECRET_FIELDS: &[&str] = &[
    "anon key",
    "service_role key",
    "JWT secret",
    "S3 Access Key",
    "S3 Secret Key",
    "Publishable",
    "Secret",
];

/// Strip the trailing version upgrade nag that Supabase CLI appends.
///
/// Example nag lines:
/// ```text
/// A new version of Supabase CLI is available: v1.x.x (currently installed v1.y.y)
/// Update by running: brew upgrade supabase
/// ```
fn strip_version_nag(output: &str) -> &str {
    let trimmed = output.trim_end();
    if trimmed.is_empty() {
        return trimmed;
    }

    // Walk backward to find where the nag starts
    let lines: Vec<&str> = trimmed.lines().collect();
    let mut cut_from = lines.len();

    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.starts_with("A new version of Supabase CLI")
            || line.starts_with("Update by running:")
            || line.starts_with("We recommend updating")
        {
            cut_from = i;
        } else if line.is_empty() && cut_from < lines.len() {
            // Allow blank lines immediately before the nag block
            cut_from = i;
        } else {
            break;
        }
    }

    if cut_from == lines.len() {
        return trimmed;
    }

    // Find the byte offset where cut_from begins
    let mut byte_offset = 0;
    for (i, line) in trimmed.lines().enumerate() {
        if i == cut_from {
            break;
        }
        byte_offset += line.len() + 1; // +1 for newline
    }

    trimmed[..byte_offset].trim_end()
}

/// Check if a status field name is a secret that should be masked.
fn is_secret_status_field(field: &str) -> bool {
    let field_lower = field.trim().to_lowercase();
    // Match exact names or names containing secret-related keywords
    STATUS_SECRET_FIELDS
        .iter()
        .any(|s| field_lower == s.to_lowercase())
        || field_lower.contains("secret")
        || field_lower.contains("access key")
}

/// Check if a line is a box-drawing border (╭╮╰╯├┤─ etc.)
fn is_box_border(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| "╭╮╰╯├┤┬┴─│┼ \t".contains(c))
}

/// Check if a line is a section header (contains emoji + title text)
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    // Section headers are inside box borders like "│ 🔧 Development Tools │"
    // They contain emoji and title text but no key-value pair
    if !trimmed.contains('│') {
        return false;
    }
    let inner = trimmed.trim_matches('│').trim();
    // Has emoji but no actual key-value structure (no │ splitting into 2+ meaningful parts)
    let parts: Vec<&str> = trimmed.split('│').map(|s| s.trim()).collect();
    let meaningful: Vec<&&str> = parts.iter().filter(|p| !p.is_empty()).collect();
    meaningful.len() == 1 && !inner.is_empty() && !inner.contains("http") && !inner.contains("://")
}

/// Filter `supabase status` output.
/// Handles both old "key: value" format and new box-drawn table format.
/// Masks secrets, keeps URLs and service info.
pub fn filter_supabase_status(output: &str, exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);

    if exit_code != 0 {
        return cleaned.to_string();
    }

    let mut lines = Vec::new();

    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip box borders
        if is_box_border(trimmed) {
            continue;
        }

        // Skip section headers
        if is_section_header(trimmed) {
            continue;
        }

        // New box format: "│ Key │ Value │"
        if trimmed.contains('│') {
            let parts: Vec<&str> = trimmed.split('│').map(|s| s.trim()).collect();
            let meaningful: Vec<&str> = parts.into_iter().filter(|p| !p.is_empty()).collect();
            if meaningful.len() >= 2 {
                let key = meaningful[0];
                let value = meaningful[1];
                if is_secret_status_field(key) {
                    lines.push(format!("{key}: ***"));
                } else {
                    lines.push(format!("{key}: {value}"));
                }
                continue;
            } else if meaningful.len() == 1 {
                // Single-value row (like status messages)
                lines.push(meaningful[0].to_string());
                continue;
            }
            continue;
        }

        // Old format: "key: value" (colon-separated)
        if let Some(colon_pos) = trimmed.find(": ") {
            let key = trimmed[..colon_pos].trim();
            let value = trimmed[colon_pos + 2..].trim();

            if is_secret_status_field(key) {
                lines.push(format!("{key}: ***"));
            } else {
                lines.push(format!("{key}: {value}"));
            }
        } else {
            // Keep non-table lines (like "Stopped services: [...]" or status messages)
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        "No status information.".to_string()
    } else {
        lines.join("\n")
    }
}

/// Filter `supabase migration list` output.
/// Strips preamble and table decorations, keeps migration entries.
pub fn filter_supabase_migration_list(output: &str, exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);

    if exit_code != 0 {
        return cleaned.to_string();
    }

    let mut migrations = Vec::new();

    for line in cleaned.lines() {
        let trimmed = line.trim();

        // Skip preamble/noise
        if trimmed.is_empty()
            || trimmed.starts_with("Connecting")
            || trimmed.starts_with("Initialising")
            || trimmed.starts_with("Listing")
        {
            continue;
        }

        // Skip header lines
        if (trimmed.contains("LOCAL") && trimmed.contains("REMOTE"))
            || (trimmed.contains("Local") && trimmed.contains("Remote"))
            || (trimmed.contains("TIME") || trimmed.contains("Time"))
                && (trimmed.contains("LOCAL")
                    || trimmed.contains("Local")
                    || trimmed.contains("REMOTE")
                    || trimmed.contains("Remote"))
        {
            continue;
        }

        // Skip table border/separator lines (─, │ only, ┼, -, +, |, etc.)
        if trimmed.chars().all(|c| "─│┼┤├┌┐└┘ \t-+|".contains(c)) {
            continue;
        }

        // Handle pipe-separated rows (both │ and | formats)
        let sep = if trimmed.contains('│') {
            '│'
        } else if trimmed.contains('|') {
            '|'
        } else {
            '\0'
        };

        if sep != '\0' {
            let parts: Vec<&str> = trimmed.split(sep).map(|s| s.trim()).collect();
            let meaningful: Vec<&str> = parts.into_iter().filter(|p| !p.is_empty()).collect();
            if !meaningful.is_empty() {
                migrations.push(meaningful.join(" | "));
            }
        } else if !trimmed.is_empty() {
            migrations.push(trimmed.to_string());
        }
    }

    if migrations.is_empty() {
        "No migrations.".to_string()
    } else {
        migrations.join("\n")
    }
}

/// Filter `supabase db diff` output.
/// Strips all preamble noise and keeps only the SQL diff content.
pub fn filter_supabase_db_diff(output: &str, exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);

    if exit_code != 0 {
        return cleaned.to_string();
    }

    let mut sql_lines = Vec::new();
    let mut found_sql = false;

    for line in cleaned.lines() {
        let trimmed = line.trim();

        if !found_sql {
            // Skip preamble lines
            if trimmed.is_empty()
                || trimmed.starts_with("Connecting")
                || trimmed.starts_with("NOTICE")
                || trimmed.starts_with("Initialising")
                || trimmed.starts_with("Seeding")
                || trimmed.contains("Applying migration")
                || trimmed.contains("Creating shadow database")
                || trimmed.contains("Diffing")
            {
                continue;
            }
            // First non-preamble line — start of SQL
            found_sql = true;
        }

        sql_lines.push(line.to_string());
    }

    // Trim trailing empty lines
    while sql_lines.last().is_some_and(|l| l.trim().is_empty()) {
        sql_lines.pop();
    }

    if sql_lines.is_empty() {
        "No schema changes.".to_string()
    } else {
        sql_lines.join("\n")
    }
}

/// Filter `supabase db reset` output.
/// Strips progress/NOTICE lines, keeps final status or error messages.
pub fn filter_supabase_db_reset(output: &str, exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);

    if exit_code != 0 {
        return cleaned.to_string();
    }

    let mut result_lines = Vec::new();

    for line in cleaned.lines() {
        let trimmed = line.trim();

        // Skip noise
        if trimmed.is_empty()
            || trimmed.starts_with("NOTICE")
            || trimmed.starts_with("Resetting")
            || trimmed.starts_with("Applying")
            || trimmed.starts_with("Creating")
            || trimmed.starts_with("Dropping")
            || trimmed.starts_with("Setting")
            || trimmed.starts_with("Initialising")
            || trimmed.starts_with("Seeding")
        {
            continue;
        }

        result_lines.push(trimmed.to_string());
    }

    if result_lines.is_empty() {
        "Database reset completed.".to_string()
    } else {
        result_lines.join("\n")
    }
}

/// Filter `supabase db push` output.
/// Similar to db reset — strip progress, keep status/errors.
pub fn filter_supabase_db_push(output: &str, exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);

    if exit_code != 0 {
        return cleaned.to_string();
    }

    let mut result_lines = Vec::new();

    for line in cleaned.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty()
            || trimmed.starts_with("Connecting")
            || trimmed.starts_with("NOTICE")
            || trimmed.starts_with("Applying")
            || trimmed.starts_with("Setting")
        {
            continue;
        }

        result_lines.push(trimmed.to_string());
    }

    if result_lines.is_empty() {
        "Database push completed.".to_string()
    } else {
        result_lines.join("\n")
    }
}

/// Filter `supabase start` and `supabase stop` output.
/// Strips Docker pull progress and container creation noise.
pub fn filter_supabase_lifecycle(output: &str, exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);

    if exit_code != 0 {
        return cleaned.to_string();
    }

    let mut result_lines = Vec::new();

    for line in cleaned.lines() {
        let trimmed = line.trim();

        // Skip Docker pull noise
        if trimmed.is_empty()
            || trimmed.starts_with("Pulling")
            || trimmed.starts_with("Creating")
            || trimmed.starts_with("Starting")
            || trimmed.starts_with("Stopping")
            || trimmed.starts_with("Waiting")
            || trimmed.contains("Pull complete")
            || trimmed.contains("Already exists")
            || trimmed.contains("Digest:")
            || trimmed.contains("Status:")
            || trimmed.contains("Downloading")
            || trimmed.contains("Extracting")
        {
            continue;
        }

        // Keep final status messages and errors
        result_lines.push(trimmed.to_string());
    }

    if result_lines.is_empty() {
        "Supabase lifecycle operation completed.".to_string()
    } else {
        result_lines.join("\n")
    }
}

/// Generic catch-all filter for `supabase` commands.
/// Strips version nag and trims whitespace.
pub fn filter_supabase_generic(output: &str, _exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- strip_version_nag tests --

    #[test]
    fn strip_nag_removes_trailing_nag() {
        let input = "some output\n\nA new version of Supabase CLI is available: v1.200.0 (currently installed v1.190.0)\nUpdate by running: brew upgrade supabase";
        let result = strip_version_nag(input);
        assert_eq!(result, "some output");
    }

    #[test]
    fn strip_nag_preserves_clean_output() {
        let input = "clean output\nno nag here";
        let result = strip_version_nag(input);
        assert_eq!(result, "clean output\nno nag here");
    }

    #[test]
    fn strip_nag_handles_empty() {
        assert_eq!(strip_version_nag(""), "");
    }

    #[test]
    fn strip_nag_only_nag() {
        let input = "A new version of Supabase CLI is available: v1.200.0 (currently installed v1.190.0)\nUpdate by running: brew upgrade supabase";
        let result = strip_version_nag(input);
        assert_eq!(result, "");
    }

    // -- status tests --

    #[test]
    fn status_extracts_urls_and_masks_secrets() {
        let input = "         API URL: http://127.0.0.1:54321
     GraphQL URL: http://127.0.0.1:54321/graphql/v1
  S3 Storage URL: http://127.0.0.1:54321/storage/v1/s3
          DB URL: postgresql://postgres:postgres@127.0.0.1:54322/postgres
      Studio URL: http://127.0.0.1:54323
    Inbucket URL: http://127.0.0.1:54324
      JWT secret: super-secret-jwt-token-with-at-least-32-characters-long
        anon key: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.fake
service_role key: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.fake2
   S3 Access Key: 625729a08b95bf1b7ff351a663f3a23c
   S3 Secret Key: 850181e4652dd023b7a98c58ae0d2d34bd487ee0cc3254aed6eda37307425907
       S3 Region: local";

        let result = filter_supabase_status(input, 0);

        // URLs should be kept
        assert!(result.contains("API URL: http://127.0.0.1:54321"));
        assert!(result.contains("GraphQL URL: http://127.0.0.1:54321/graphql/v1"));
        assert!(result.contains("DB URL: postgresql://postgres:postgres@127.0.0.1:54322/postgres"));
        assert!(result.contains("S3 Region: local"));

        // Secrets should be masked
        assert!(result.contains("JWT secret: ***"));
        assert!(result.contains("anon key: ***"));
        assert!(result.contains("service_role key: ***"));
        assert!(result.contains("S3 Access Key: ***"));
        assert!(result.contains("S3 Secret Key: ***"));

        // Actual secret values should not appear
        assert!(!result.contains("super-secret-jwt-token"));
        assert!(!result.contains("eyJhbGci"));
        assert!(!result.contains("625729a08b95"));
        assert!(!result.contains("850181e4652d"));
    }

    #[test]
    fn status_box_format_masks_secrets() {
        let input = r#"╭──────────────────────────────────────╮
│ 🔧 Development Tools                 │
├─────────┬────────────────────────────┤
│ Studio  │ http://127.0.0.1:54323     │
│ Mailpit │ http://127.0.0.1:54324     │
╰─────────┴────────────────────────────╯
╭─────────────────────────────────────────────────╮
│ 🌐 APIs                                         │
├─────────────┬───────────────────────────────────┤
│ Project URL │ http://127.0.0.1:54321            │
╰─────────────┴───────────────────────────────────╯
╭───────────────────────────────────────────────────────────────╮
│ ⛁ Database                                                    │
├─────┬─────────────────────────────────────────────────────────┤
│ URL │ postgresql://postgres:postgres@127.0.0.1:54322/postgres │
╰─────┴─────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────────╮
│ 🔑 Authentication Keys                                       │
├─────────────┬────────────────────────────────────────────────┤
│ Publishable │ sb_publishable_FAKE_TEST_VALUE_000000000000 │
│ Secret      │ sb_secret_FAKE_TEST_VALUE_0000000000000      │
╰─────────────┴────────────────────────────────────────────────╯
╭───────────────────────────────────────────────────────────────────────────────╮
│ 📦 Storage (S3)                                                               │
├────────────┬──────────────────────────────────────────────────────────────────┤
│ URL        │ http://127.0.0.1:54321/storage/v1/s3                             │
│ Access Key │ 625729a08b95bf1b7ff351a663f3a23c                                 │
│ Secret Key │ 850181e4652dd023b7a98c58ae0d2d34bd487ee0cc3254aed6eda37307425907 │
│ Region     │ local                                                            │
╰────────────┴──────────────────────────────────────────────────────────────────╯
Stopped services: [supabase_imgproxy_main]
supabase local development setup is running."#;

        let result = filter_supabase_status(input, 0);

        // URLs should be kept
        assert!(
            result.contains("Studio: http://127.0.0.1:54323"),
            "got: {result}"
        );
        assert!(
            result.contains("Project URL: http://127.0.0.1:54321"),
            "got: {result}"
        );
        assert!(
            result.contains("URL: postgresql://postgres:postgres@127.0.0.1:54322/postgres"),
            "got: {result}"
        );

        // Secrets should be masked
        assert!(result.contains("Publishable: ***"), "got: {result}");
        assert!(result.contains("Secret: ***"), "got: {result}");
        assert!(result.contains("Access Key: ***"), "got: {result}");
        assert!(result.contains("Secret Key: ***"), "got: {result}");

        // Actual secret values should NOT appear
        assert!(!result.contains("sb_publishable_FAKE"), "got: {result}");
        assert!(!result.contains("sb_secret_FAKE"), "got: {result}");
        assert!(!result.contains("625729a08b95"), "got: {result}");
        assert!(!result.contains("850181e4652d"), "got: {result}");

        // Non-secret values kept
        assert!(result.contains("Region: local"), "got: {result}");
        assert!(result.contains("Stopped services:"), "status msg: {result}");

        // Box drawing should be gone
        assert!(!result.contains('╭'), "got: {result}");
        assert!(!result.contains('╰'), "got: {result}");
    }

    #[test]
    fn status_with_nag() {
        let input = "         API URL: http://127.0.0.1:54321\n      JWT secret: my-secret\n\nA new version of Supabase CLI is available: v1.200.0 (currently installed v1.190.0)\nUpdate by running: brew upgrade supabase";

        let result = filter_supabase_status(input, 0);
        assert!(result.contains("API URL: http://127.0.0.1:54321"));
        assert!(result.contains("JWT secret: ***"));
        assert!(!result.contains("new version"));
    }

    #[test]
    fn status_error_passthrough() {
        let input = "Error: Cannot connect to local Supabase.";
        let result = filter_supabase_status(input, 1);
        assert_eq!(result, "Error: Cannot connect to local Supabase.");
    }

    // -- migration list tests --

    #[test]
    fn migration_list_parses_entries_pipe_format() {
        let input = "Initialising login role...\nConnecting to remote database...\n\n  \n   Local          | Remote         | Time (UTC)          \n  ----------------|----------------|---------------------\n   001            | 001            | 001                 \n   002            | 002            | 002                 ";

        let result = filter_supabase_migration_list(input, 0);
        assert!(result.contains("001"), "got: {result}");
        assert!(result.contains("002"), "got: {result}");
        assert!(!result.contains("Connecting"), "got: {result}");
        assert!(!result.contains("Initialising"), "got: {result}");
    }

    #[test]
    fn migration_list_parses_entries_unicode() {
        let input = "Connecting to linked project...\nInitialising...\n        LOCAL      │     REMOTE     │     TIME (UTC)\n  ─────────────────┼────────────────┼──────────────────────\n  20240101000000   │ 20240101000000 │ 2024-01-01 00:00:00\n  20240215120000   │ 20240215120000 │ 2024-02-15 12:00:00";

        let result = filter_supabase_migration_list(input, 0);
        assert!(result.contains("20240101000000"));
        assert!(result.contains("20240215120000"));
        assert!(!result.contains("Connecting"));
        assert!(!result.contains("Initialising"));
    }

    #[test]
    fn migration_list_empty() {
        let input = "Connecting to linked project...\nInitialising...\n        LOCAL      │     REMOTE     │     TIME (UTC)\n  ─────────────────┼────────────────┼──────────────────────";

        let result = filter_supabase_migration_list(input, 0);
        assert_eq!(result, "No migrations.");
    }

    #[test]
    fn migration_list_error() {
        let input = "Error: Access token not found.";
        let result = filter_supabase_migration_list(input, 1);
        assert_eq!(result, "Error: Access token not found.");
    }

    // -- db diff tests --

    #[test]
    fn db_diff_strips_preamble_keeps_sql() {
        let input = "Connecting to local database...\nCreating shadow database...\nNOTICE: extension \"pg_graphql\" is not available\nDiffing schemas: public\n\nCREATE TABLE public.users (\n    id uuid PRIMARY KEY,\n    name text NOT NULL\n);";

        let result = filter_supabase_db_diff(input, 0);
        assert!(result.starts_with("CREATE TABLE"));
        assert!(result.contains("id uuid PRIMARY KEY"));
        assert!(!result.contains("Connecting"));
        assert!(!result.contains("NOTICE"));
        assert!(!result.contains("Creating shadow"));
        assert!(!result.contains("Diffing"));
    }

    #[test]
    fn db_diff_no_changes() {
        let input = "Connecting to local database...\nCreating shadow database...\nNOTICE: extension \"pg_graphql\" is not available\nDiffing schemas: public\n";

        let result = filter_supabase_db_diff(input, 0);
        assert_eq!(result, "No schema changes.");
    }

    #[test]
    fn db_diff_error() {
        let input = "Error: could not connect to database";
        let result = filter_supabase_db_diff(input, 1);
        assert_eq!(result, "Error: could not connect to database");
    }

    // -- db reset tests --

    #[test]
    fn db_reset_strips_progress() {
        let input = "Resetting local database...\nDropping local database...\nCreating local database...\nApplying migration 20240101000000...\nNOTICE: something\nSetting up initial schema...\nSeeding data...\nFinished supabase db reset on local database.";

        let result = filter_supabase_db_reset(input, 0);
        assert_eq!(result, "Finished supabase db reset on local database.");
        assert!(!result.contains("Resetting"));
        assert!(!result.contains("NOTICE"));
    }

    #[test]
    fn db_reset_empty_success() {
        let input = "Resetting local database...\nDropping local database...\nCreating local database...\nApplying migration 20240101000000...\nNOTICE: something\nSetting up initial schema...\nSeeding data...";

        let result = filter_supabase_db_reset(input, 0);
        assert_eq!(result, "Database reset completed.");
    }

    #[test]
    fn db_reset_error() {
        let input = "Error: permission denied for schema public";
        let result = filter_supabase_db_reset(input, 1);
        assert_eq!(result, "Error: permission denied for schema public");
    }

    // -- lifecycle tests --

    #[test]
    fn lifecycle_start_keeps_final_message() {
        let input = "Pulling images...\nPulling supabase/postgres:15.1.1.2...\nDigest: sha256:abc123\nStatus: Image is up to date\nCreating supabase_db_1...\nCreating supabase_auth_1...\nStarting supabase_db_1...\nWaiting for health checks...\nStarted supabase local development setup.";

        let result = filter_supabase_lifecycle(input, 0);
        assert_eq!(result, "Started supabase local development setup.");
    }

    #[test]
    fn lifecycle_stop_keeps_final_message() {
        let input = "Stopping containers...\nStopped supabase local development setup.";

        let result = filter_supabase_lifecycle(input, 0);
        assert_eq!(result, "Stopped supabase local development setup.");
    }

    #[test]
    fn lifecycle_error() {
        let input = "Error: Cannot connect to Docker daemon";
        let result = filter_supabase_lifecycle(input, 1);
        assert_eq!(result, "Error: Cannot connect to Docker daemon");
    }

    // -- generic tests --

    #[test]
    fn generic_strips_nag_only() {
        let input = "Usage: supabase [command]\n\nAvailable Commands:\n  start       Start containers\n  stop        Stop containers\n\nA new version of Supabase CLI is available: v1.200.0 (currently installed v1.190.0)\nUpdate by running: brew upgrade supabase";

        let result = filter_supabase_generic(input, 0);
        assert!(result.contains("Usage: supabase [command]"));
        assert!(result.contains("Available Commands:"));
        assert!(!result.contains("new version"));
    }

    #[test]
    fn generic_empty() {
        let result = filter_supabase_generic("", 0);
        assert_eq!(result, "");
    }

    // -- db push tests --

    #[test]
    fn db_push_strips_progress() {
        let input = "Connecting to remote database...\nNOTICE: something\nApplying migration 20240101000000...\nSetting up initial schema...\nFinished supabase db push.";

        let result = filter_supabase_db_push(input, 0);
        assert_eq!(result, "Finished supabase db push.");
    }

    #[test]
    fn db_push_empty_success() {
        let input = "Connecting to remote database...\nApplying migration 20240101000000...";

        let result = filter_supabase_db_push(input, 0);
        assert_eq!(result, "Database push completed.");
    }
}
