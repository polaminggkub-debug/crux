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
/// Strips preamble noise and aggressively summarizes SQL content.
pub fn filter_supabase_db_diff(output: &str, exit_code: i32) -> String {
    let cleaned = strip_version_nag(output);

    if exit_code != 0 {
        return cleaned.to_string();
    }

    // Extract SQL content (skip preamble)
    let mut sql_lines = Vec::new();
    let mut found_sql = false;

    for line in cleaned.lines() {
        let trimmed = line.trim();

        if !found_sql {
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
            found_sql = true;
        }

        sql_lines.push(line);
    }

    // Trim trailing empty lines
    while sql_lines.last().is_some_and(|l| l.trim().is_empty()) {
        sql_lines.pop();
    }

    if sql_lines.is_empty() {
        "No schema changes.".to_string()
    } else {
        let sql = sql_lines.join("\n");
        summarize_sql(&sql)
    }
}

/// Summarize SQL diff into a compact format for AI consumption.
///
/// Parses SQL statements and reduces them:
/// - `CREATE TABLE` → table name + column names + count
/// - `ALTER TABLE ADD/DROP/ALTER COLUMN` → kept as-is
/// - `CREATE INDEX` → name + target table only
/// - `CREATE FUNCTION` → signature line only
/// - `GRANT/REVOKE` → counted and summarized
/// - `CREATE POLICY` → name + target table
/// - `ALTER TABLE ... OWNER TO` → dropped (noise)
/// - Comments and SET statements → dropped
fn summarize_sql(sql: &str) -> String {
    let mut results: Vec<String> = Vec::new();
    let mut grant_count: usize = 0;
    let lines: Vec<&str> = sql.lines().collect();
    let len = lines.len();
    let mut i = 0;

    while i < len {
        let trimmed = lines[i].trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("--") {
            i += 1;
            continue;
        }

        let upper = trimmed.to_uppercase();

        // Skip SET statements
        if upper.starts_with("SET ") {
            i = skip_to_semicolon(lines.as_slice(), i);
            continue;
        }

        // GRANT/REVOKE — just count
        if upper.starts_with("GRANT ") || upper.starts_with("REVOKE ") {
            grant_count += 1;
            i = skip_to_semicolon(lines.as_slice(), i);
            continue;
        }

        // ALTER TABLE ... OWNER TO — skip (noise)
        if upper.starts_with("ALTER TABLE ") && upper.contains("OWNER TO") {
            i = skip_to_semicolon(lines.as_slice(), i);
            continue;
        }

        // ALTER TABLE with column changes — keep as-is
        if upper.starts_with("ALTER TABLE ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            // Flatten to one line, trim excess whitespace
            let one_line = flatten_statement(&stmt);
            results.push(one_line);
            continue;
        }

        // CREATE TABLE — summarize with column names
        if upper.starts_with("CREATE TABLE ") || upper.starts_with("CREATE UNLOGGED TABLE ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            results.push(summarize_create_table(&stmt));
            continue;
        }

        // CREATE INDEX
        if upper.starts_with("CREATE INDEX ") || upper.starts_with("CREATE UNIQUE INDEX ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            results.push(summarize_create_index(&stmt));
            continue;
        }

        // CREATE [OR REPLACE] FUNCTION / PROCEDURE
        if upper.starts_with("CREATE FUNCTION ")
            || upper.starts_with("CREATE OR REPLACE FUNCTION ")
            || upper.starts_with("CREATE PROCEDURE ")
            || upper.starts_with("CREATE OR REPLACE PROCEDURE ")
        {
            // Only need the first line for signature; skip entire body
            let first_line = lines[i];
            i = skip_to_semicolon_or_dollar(lines.as_slice(), i);
            results.push(summarize_create_function(&[first_line]));
            continue;
        }

        // CREATE POLICY
        if upper.starts_with("CREATE POLICY ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            results.push(summarize_create_policy(&stmt));
            continue;
        }

        // DROP statements — keep as-is (short and important)
        if upper.starts_with("DROP ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            results.push(flatten_statement(&stmt));
            continue;
        }

        // CREATE TRIGGER — summarize
        if upper.starts_with("CREATE TRIGGER ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            results.push(summarize_create_trigger(&stmt));
            continue;
        }

        // CREATE TYPE — keep first line
        if upper.starts_with("CREATE TYPE ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            // Extract type name
            let first = stmt.first().map(|s| s.trim()).unwrap_or("");
            if let Some(name) = extract_name_after(first, "TYPE") {
                results.push(format!("CREATE TYPE {name}"));
            } else {
                results.push(flatten_statement(&stmt));
            }
            continue;
        }

        // CREATE SEQUENCE, CREATE EXTENSION, CREATE SCHEMA, etc. — keep short
        if upper.starts_with("CREATE ") {
            let stmt = collect_statement(lines.as_slice(), i);
            i = skip_to_semicolon(lines.as_slice(), i);
            results.push(flatten_statement(&stmt));
            continue;
        }

        // Anything else — keep as one line
        let stmt = collect_statement(lines.as_slice(), i);
        i = skip_to_semicolon(lines.as_slice(), i);
        results.push(flatten_statement(&stmt));
    }

    if grant_count > 0 {
        results.push(format!(
            "{grant_count} permission statement{}",
            if grant_count == 1 { "" } else { "s" }
        ));
    }

    if results.is_empty() {
        "No schema changes.".to_string()
    } else {
        results.join("\n")
    }
}

/// Collect all lines of a statement starting at `start`.
fn collect_statement<'a>(lines: &[&'a str], start: usize) -> Vec<&'a str> {
    let mut stmt = vec![lines[start]];
    // If the first line already ends with ';', it's a complete statement
    if lines[start].trim().ends_with(';') {
        return stmt;
    }
    let mut j = start + 1;
    while j < lines.len() {
        let t = lines[j].trim();
        stmt.push(lines[j]);
        if t.ends_with(';') {
            break;
        }
        j += 1;
    }
    stmt
}

/// Advance index past the current statement (to the line after the semicolon).
fn skip_to_semicolon(lines: &[&str], start: usize) -> usize {
    let mut j = start;
    while j < lines.len() {
        if lines[j].trim().ends_with(';') {
            return j + 1;
        }
        j += 1;
    }
    lines.len()
}

/// Advance past a function definition that may use $$ delimiters.
fn skip_to_semicolon_or_dollar(lines: &[&str], start: usize) -> usize {
    let mut j = start;
    let mut dollar_count = 0;
    while j < lines.len() {
        let t = lines[j].trim();
        dollar_count += t.matches("$$").count();
        // After seeing both opening and closing $$, the next ; ends it
        if dollar_count >= 2 {
            if t.ends_with(';') {
                return j + 1;
            }
            // $$; on same line as closing $$
            j += 1;
            continue;
        }
        // Only stop at ; if we haven't entered a $$ block yet
        if dollar_count == 0 && t.ends_with(';') {
            return j + 1;
        }
        j += 1;
    }
    lines.len()
}

/// Flatten a multi-line statement into a single line, collapsing whitespace.
fn flatten_statement(lines: &[&str]) -> String {
    let joined: String = lines
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
    // Collapse multiple spaces
    let mut result = String::with_capacity(joined.len());
    let mut prev_space = false;
    for c in joined.chars() {
        if c.is_whitespace() {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    // Strip trailing semicolon for cleaner output
    let r = result.trim().trim_end_matches(';').trim().to_string();
    r
}

/// Summarize CREATE TABLE into: `CREATE TABLE schema.table (col1, col2, ...) [N columns]`
fn summarize_create_table(lines: &[&str]) -> String {
    let full = lines.join("\n");

    // Extract table name from first line
    let first = lines[0].trim();
    let table_name = extract_name_after(first, "TABLE").unwrap_or("?");

    // Extract column names from between the parentheses
    let mut cols: Vec<&str> = Vec::new();
    let mut constraints: usize = 0;

    // Find content between ( and );
    let paren_start = full.find('(');
    let paren_end = full.rfind(')');

    if let (Some(start), Some(end)) = (paren_start, paren_end) {
        let body = &full[start + 1..end];
        for part in split_top_level(body) {
            let t = part.trim();
            let t_upper = t.to_uppercase();
            // Skip constraints (PRIMARY KEY, UNIQUE, CHECK, FOREIGN KEY, CONSTRAINT)
            if t_upper.starts_with("PRIMARY KEY")
                || t_upper.starts_with("UNIQUE")
                || t_upper.starts_with("CHECK")
                || t_upper.starts_with("FOREIGN KEY")
                || t_upper.starts_with("CONSTRAINT")
                || t_upper.starts_with("EXCLUDE")
            {
                constraints += 1;
                continue;
            }
            // Column name is the first word
            if let Some(name) = t.split_whitespace().next() {
                cols.push(name);
            }
        }
    }

    let col_count = cols.len();
    if col_count == 0 {
        return format!("CREATE TABLE {table_name}");
    }

    let col_list = cols.join(", ");
    let mut result = format!("CREATE TABLE {table_name} ({col_list}) [{col_count} columns]");
    if constraints > 0 {
        result.push_str(&format!(
            " [{constraints} constraint{}]",
            if constraints == 1 { "" } else { "s" }
        ));
    }
    result
}

/// Split a string by top-level commas (not inside parentheses).
fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

/// Summarize CREATE INDEX into: `CREATE INDEX name ON table`
fn summarize_create_index(lines: &[&str]) -> String {
    let flat = flatten_statement(lines);
    let upper = flat.to_uppercase();

    // Find index name and table
    let idx_name = if upper.contains("UNIQUE INDEX ") {
        extract_name_after(&flat, "UNIQUE INDEX")
    } else {
        extract_name_after(&flat, "INDEX")
    };

    let on_table = extract_name_after(&flat, "ON");

    match (idx_name, on_table) {
        (Some(name), Some(table)) => format!("CREATE INDEX {name} ON {table}"),
        (Some(name), None) => format!("CREATE INDEX {name}"),
        _ => flat,
    }
}

/// Summarize CREATE FUNCTION into: `CREATE FUNCTION schema.func_name(args)`
fn summarize_create_function(lines: &[&str]) -> String {
    let first = lines[0].trim();

    // Find the function name and args up to the closing paren
    let upper = first.to_uppercase();
    let func_pos = upper.find("FUNCTION ");
    if func_pos.is_none() {
        // Try PROCEDURE
        if let Some(pos) = upper.find("PROCEDURE ") {
            let rest = &first[pos + "PROCEDURE ".len()..];
            let sig = if let Some(p) = rest.find(')') {
                &rest[..=p]
            } else {
                rest.split_whitespace().next().unwrap_or(rest)
            };
            return format!("CREATE PROCEDURE {sig}");
        }
        return flatten_statement(lines);
    }

    let rest = &first[func_pos.unwrap() + "FUNCTION ".len()..];
    let sig = if let Some(p) = rest.find(')') {
        &rest[..=p]
    } else {
        rest.split_whitespace().next().unwrap_or(rest)
    };

    format!("CREATE FUNCTION {sig}")
}

/// Summarize CREATE POLICY into: `CREATE POLICY name ON table`
fn summarize_create_policy(lines: &[&str]) -> String {
    let flat = flatten_statement(lines);
    let policy_name = extract_name_after(&flat, "POLICY");
    let on_table = extract_name_after(&flat, "ON");

    match (policy_name, on_table) {
        (Some(name), Some(table)) => format!("CREATE POLICY {name} ON {table}"),
        (Some(name), None) => format!("CREATE POLICY {name}"),
        _ => flat,
    }
}

/// Summarize CREATE TRIGGER.
fn summarize_create_trigger(lines: &[&str]) -> String {
    let flat = flatten_statement(lines);
    let trigger_name = extract_name_after(&flat, "TRIGGER");
    let on_table = extract_name_after(&flat, "ON");

    match (trigger_name, on_table) {
        (Some(name), Some(table)) => format!("CREATE TRIGGER {name} ON {table}"),
        (Some(name), None) => format!("CREATE TRIGGER {name}"),
        _ => flat,
    }
}

/// Extract the name token after a keyword like TABLE, INDEX, ON, etc.
/// Returns the word (possibly schema-qualified) immediately after the keyword.
fn extract_name_after<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    let upper = s.to_uppercase();
    let kw_upper = keyword.to_uppercase();
    let search = format!("{kw_upper} ");

    let pos = upper.find(&search)?;
    let after = &s[pos + search.len()..];
    let after = after.trim_start();

    // Skip common noise words
    let word = after.split_whitespace().next()?;
    let w_upper = word.to_uppercase();
    if w_upper == "IF"
        || w_upper == "NOT"
        || w_upper == "EXISTS"
        || w_upper == "ONLY"
        || w_upper == "OR"
    {
        // Skip "IF NOT EXISTS" or "ONLY"
        let rest = &after[word.len()..].trim_start();
        if w_upper == "IF" {
            // Skip "IF NOT EXISTS"
            let rest2 = rest
                .strip_prefix("NOT")
                .unwrap_or(rest)
                .trim_start()
                .strip_prefix("EXISTS")
                .unwrap_or(rest)
                .trim_start();
            return rest2.split_whitespace().next();
        }
        return rest.split_whitespace().next();
    }

    // Strip trailing punctuation
    let name = word.trim_end_matches(['(', ')', ';', ',']);
    if name.is_empty() {
        None
    } else {
        Some(name)
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
    fn db_diff_strips_preamble_and_summarizes_sql() {
        let input = "Connecting to local database...\nCreating shadow database...\nNOTICE: extension \"pg_graphql\" is not available\nDiffing schemas: public\n\nCREATE TABLE public.users (\n    id uuid DEFAULT gen_random_uuid() NOT NULL,\n    name text NOT NULL\n);";

        let result = filter_supabase_db_diff(input, 0);
        assert!(
            result.contains("CREATE TABLE public.users"),
            "got: {result}"
        );
        assert!(result.contains("id"), "should list column names: {result}");
        assert!(result.contains("name"), "should list column names: {result}");
        assert!(result.contains("[2 columns]"), "got: {result}");
        // Should NOT contain full DDL details
        assert!(
            !result.contains("gen_random_uuid"),
            "should not have defaults: {result}"
        );
        assert!(!result.contains("Connecting"), "got: {result}");
        assert!(!result.contains("NOTICE"), "got: {result}");
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

    #[test]
    fn db_diff_create_table_summarization() {
        let sql = "CREATE TABLE public.users (\n    id uuid DEFAULT gen_random_uuid() NOT NULL,\n    name text NOT NULL,\n    email text NOT NULL,\n    created_at timestamp with time zone DEFAULT now() NOT NULL,\n    updated_at timestamp with time zone DEFAULT now() NOT NULL\n);";

        let result = summarize_sql(sql);
        assert!(
            result.contains("CREATE TABLE public.users"),
            "got: {result}"
        );
        assert!(result.contains("id"), "got: {result}");
        assert!(result.contains("email"), "got: {result}");
        assert!(result.contains("[5 columns]"), "got: {result}");
        assert!(
            !result.contains("gen_random_uuid"),
            "should strip defaults: {result}"
        );
        assert!(
            !result.contains("timestamp with time zone"),
            "should strip types: {result}"
        );
    }

    #[test]
    fn db_diff_mixed_statements() {
        let sql = "\
CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    email text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE public.users OWNER TO postgres;

CREATE INDEX idx_users_email ON public.users USING btree (email);

ALTER TABLE public.orders ADD COLUMN shipping_address text;

GRANT ALL ON TABLE public.users TO authenticated;
GRANT SELECT ON TABLE public.users TO anon;";

        let result = summarize_sql(sql);

        // CREATE TABLE summarized
        assert!(
            result.contains("CREATE TABLE public.users"),
            "got: {result}"
        );
        assert!(result.contains("[5 columns]"), "got: {result}");

        // OWNER TO dropped
        assert!(!result.contains("OWNER TO"), "got: {result}");

        // CREATE INDEX summarized
        assert!(
            result.contains("CREATE INDEX idx_users_email ON public.users"),
            "got: {result}"
        );
        assert!(
            !result.contains("USING btree"),
            "should strip USING clause: {result}"
        );

        // ALTER TABLE kept
        assert!(
            result.contains("ALTER TABLE public.orders ADD COLUMN shipping_address text"),
            "got: {result}"
        );

        // GRANTs counted
        assert!(
            result.contains("2 permission statements"),
            "got: {result}"
        );
    }

    #[test]
    fn db_diff_create_function_summarized() {
        let sql = "\
CREATE OR REPLACE FUNCTION public.handle_new_user() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    AS $$
BEGIN
    INSERT INTO public.profiles (id, email)
    VALUES (NEW.id, NEW.email);
    RETURN NEW;
END;
$$;";

        let result = summarize_sql(sql);
        assert!(
            result.contains("CREATE FUNCTION public.handle_new_user()"),
            "got: {result}"
        );
        // Should not contain function body
        assert!(
            !result.contains("INSERT INTO"),
            "should not have body: {result}"
        );
        assert!(
            !result.contains("RETURN NEW"),
            "should not have body: {result}"
        );
    }

    #[test]
    fn db_diff_create_policy_summarized() {
        let sql = "\
CREATE POLICY \"Users can view own data\" ON public.users
    FOR SELECT
    USING (auth.uid() = id);";

        let result = summarize_sql(sql);
        assert!(
            result.contains("CREATE POLICY"),
            "got: {result}"
        );
        assert!(
            result.contains("ON public.users"),
            "got: {result}"
        );
        // Should not contain the USING clause
        assert!(
            !result.contains("auth.uid()"),
            "should not have policy body: {result}"
        );
    }

    #[test]
    fn db_diff_high_compression_ratio() {
        // Simulate a realistic ~1KB input that should compress well
        let sql = "\
CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    email text NOT NULL,
    avatar_url text,
    bio text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT users_pkey PRIMARY KEY (id),
    CONSTRAINT users_email_key UNIQUE (email)
);

ALTER TABLE public.users OWNER TO postgres;

CREATE INDEX idx_users_email ON public.users USING btree (email);
CREATE INDEX idx_users_created_at ON public.users USING btree (created_at);

ALTER TABLE public.orders ADD COLUMN shipping_address text;
ALTER TABLE public.orders DROP COLUMN old_field;

GRANT ALL ON TABLE public.users TO authenticated;
GRANT SELECT ON TABLE public.users TO anon;
GRANT ALL ON TABLE public.orders TO authenticated;
GRANT SELECT ON TABLE public.orders TO anon;";

        let result = summarize_sql(sql);
        let input_len = sql.len();
        let output_len = result.len();
        let savings = 1.0 - (output_len as f64 / input_len as f64);

        assert!(
            savings > 0.5,
            "Expected >50% savings, got {:.1}% (input={input_len}, output={output_len})\nResult:\n{result}",
            savings * 100.0
        );
    }

    #[test]
    fn db_diff_drop_statements_kept() {
        let sql = "DROP TABLE IF EXISTS public.old_table;\nDROP INDEX IF EXISTS idx_old;";
        let result = summarize_sql(sql);
        assert!(
            result.contains("DROP TABLE IF EXISTS public.old_table"),
            "got: {result}"
        );
        assert!(
            result.contains("DROP INDEX IF EXISTS idx_old"),
            "got: {result}"
        );
    }

    #[test]
    fn db_diff_create_table_with_constraints() {
        let sql = "\
CREATE TABLE public.orders (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    total numeric(10,2) NOT NULL,
    CONSTRAINT orders_pkey PRIMARY KEY (id),
    CONSTRAINT orders_user_fk FOREIGN KEY (user_id) REFERENCES public.users(id)
);";

        let result = summarize_sql(sql);
        assert!(
            result.contains("CREATE TABLE public.orders (id, user_id, total) [3 columns]"),
            "got: {result}"
        );
        assert!(
            result.contains("[2 constraints]"),
            "got: {result}"
        );
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
