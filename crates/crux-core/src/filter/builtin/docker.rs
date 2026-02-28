use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register Docker command handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("docker ps", filter_docker_ps as BuiltinFilterFn);
    m.insert("docker images", filter_docker_images as BuiltinFilterFn);
    m.insert("docker logs", filter_docker_logs as BuiltinFilterFn);
    m.insert("docker compose", filter_docker_compose as BuiltinFilterFn);
    m.insert(
        "docker compose logs",
        filter_docker_compose_logs as BuiltinFilterFn,
    );
    m.insert(
        "docker-compose logs",
        filter_docker_compose_logs as BuiltinFilterFn,
    );
}

/// Filter docker ps: keep header + container lines, strip PORTS column, show NAME, STATUS, IMAGE.
pub fn filter_docker_ps(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return "No containers.".to_string();
    }

    let header = lines[0];
    let col_positions = parse_column_positions(header);

    // Find PORTS column to strip
    let ports_idx = col_positions.iter().position(|c| c.name == "PORTS");

    let mut result = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            let filtered = strip_column(line, &col_positions, ports_idx);
            result.push(filtered);
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let filtered = strip_column(line, &col_positions, ports_idx);
        result.push(filtered);
    }

    if result.len() <= 1 {
        "No containers.".to_string()
    } else {
        result.join("\n")
    }
}

/// Filter docker images: keep header + image lines, strip IMAGE ID column.
pub fn filter_docker_images(output: &str, _exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return "No images.".to_string();
    }

    let header = lines[0];
    let col_positions = parse_column_positions(header);

    // Strip "IMAGE ID" column specifically
    let strip_idx = col_positions.iter().position(|c| c.name == "IMAGE ID");

    let mut result = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            let filtered = strip_column(line, &col_positions, strip_idx);
            result.push(filtered);
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let filtered = strip_column(line, &col_positions, strip_idx);
        result.push(filtered);
    }

    if result.len() <= 1 {
        "No images.".to_string()
    } else {
        result.join("\n")
    }
}

/// Filter docker logs: if > 100 lines, show last 50 with summary. Strip timestamp prefixes.
pub fn filter_docker_logs(output: &str, _exit_code: i32) -> String {
    if output.trim().is_empty() {
        return "No log output.".to_string();
    }

    let timestamp_re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[.\d]*Z?\s*").unwrap();

    let all_lines: Vec<&str> = output.lines().collect();
    let total = all_lines.len();

    let lines_to_show: Vec<String> = if total > 100 {
        let tail = &all_lines[total - 50..];
        tail.iter()
            .map(|l| strip_timestamp(l, &timestamp_re))
            .collect()
    } else {
        all_lines
            .iter()
            .map(|l| strip_timestamp(l, &timestamp_re))
            .collect()
    };

    let mut result = Vec::new();

    if total > 100 {
        result.push(format!("... ({total} total lines, showing last 50)"));
    }

    for line in &lines_to_show {
        result.push(line.clone());
    }

    result.join("\n")
}

/// Filter docker compose: keep service status and Container Started/Stopped lines.
/// Drop pull progress, build output, and verbose noise.
pub fn filter_docker_compose(output: &str, exit_code: i32) -> String {
    let container_action_re = Regex::new(
        r"(?i)container\s+\S+\s+(started|stopped|created|removed|running|healthy|exited)",
    )
    .unwrap();
    let service_status_re = Regex::new(r"(?i)^\s*(name|service)\s+").unwrap();
    let compose_status_line_re =
        Regex::new(r"(?i)^\s*\S+\s+\S+\s+(running|exited|restarting|created|dead|paused)").unwrap();

    let mut result = Vec::new();
    let mut seen_header = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Keep error/warning lines
        if trimmed.starts_with("Error")
            || trimmed.starts_with("error")
            || trimmed.starts_with("WARNING")
            || trimmed.starts_with("warning")
        {
            result.push(trimmed.to_string());
            continue;
        }

        // Keep "Container X Started/Stopped/..." action lines
        if container_action_re.is_match(trimmed) {
            result.push(trimmed.to_string());
            continue;
        }

        // Keep header line for `docker compose ps` output
        if !seen_header && service_status_re.is_match(trimmed) {
            result.push(trimmed.to_string());
            seen_header = true;
            continue;
        }

        // Keep service status lines (name  image  status pattern)
        if seen_header && compose_status_line_re.is_match(trimmed) {
            result.push(trimmed.to_string());
            continue;
        }

        // Keep "done" / "up-to-date" lines
        if trimmed.ends_with("done") || trimmed.contains("up-to-date") {
            result.push(trimmed.to_string());
            continue;
        }

        // Drop everything else: pull progress, build steps, layer downloads, etc.
    }

    if result.is_empty() {
        if exit_code != 0 {
            format!("Docker compose failed (exit code {exit_code}).")
        } else {
            "Docker compose completed.".to_string()
        }
    } else {
        result.join("\n")
    }
}

/// Filter docker compose logs: strip timestamps, deduplicate container prefixes,
/// keep error/warning lines, truncate if > 200 lines.
pub fn filter_docker_compose_logs(output: &str, _exit_code: i32) -> String {
    if output.trim().is_empty() {
        return "No log output.".to_string();
    }

    let timestamp_re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[.\d]*Z?\s*").unwrap();
    let container_prefix_re = Regex::new(r"^(\S+\s*\| ?)").unwrap();

    let raw_lines: Vec<&str> = output.lines().collect();
    let cleaned = dedupe_container_prefixes(&raw_lines, &timestamp_re, &container_prefix_re);

    if cleaned.len() <= 200 {
        return cleaned.join("\n");
    }

    let total = cleaned.len();
    let omitted = total - 50 - 50;
    let mut result: Vec<&str> = cleaned[..50].iter().map(|s| s.as_str()).collect();
    result.push("");
    let msg = format!("...{omitted} lines omitted...");
    let mut out = result.join("\n");
    out.push('\n');
    out.push_str(&msg);
    out.push('\n');
    for line in &cleaned[total - 50..] {
        out.push_str(line);
        out.push('\n');
    }
    // Remove trailing newline
    out.truncate(out.trim_end().len());
    out
}

/// Strip timestamps from lines and deduplicate consecutive container name prefixes.
fn dedupe_container_prefixes(
    lines: &[&str],
    timestamp_re: &Regex,
    container_re: &Regex,
) -> Vec<String> {
    let mut result = Vec::with_capacity(lines.len());
    let mut last_container: Option<String> = None;

    for line in lines {
        // First try stripping timestamp at the start of the line
        let no_ts = strip_timestamp(line, timestamp_re);
        if let Some(caps) = container_re.captures(&no_ts) {
            let prefix = caps.get(1).unwrap().as_str().to_string();
            let rest = &no_ts[prefix.len()..];
            // Strip timestamp that may appear after the container prefix
            let rest_clean = strip_timestamp(rest, timestamp_re);
            if last_container.as_deref() == Some(&prefix) {
                result.push(format!("  {rest_clean}"));
            } else {
                last_container = Some(prefix.clone());
                result.push(format!("{prefix}{rest_clean}"));
            }
        } else {
            last_container = None;
            result.push(no_ts);
        }
    }
    result
}

// -- helpers --

struct ColumnDef {
    name: String,
    start: usize,
    end: usize, // exclusive, or usize::MAX for last column
}

/// Parse column positions from a Docker-style header line.
/// Docker uses fixed-width columns separated by 2+ spaces.
/// Column names like "CONTAINER ID" or "IMAGE ID" contain single spaces.
fn parse_column_positions(header: &str) -> Vec<ColumnDef> {
    let mut cols = Vec::new();

    // Split on 2+ spaces to find column name tokens and their positions
    let mut matches: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    let bytes = header.as_bytes();
    let len = bytes.len();

    while i < len {
        // Skip leading spaces
        if bytes[i] == b' ' {
            i += 1;
            continue;
        }

        // Found start of a column name
        let start = i;
        // Read until we hit 2+ consecutive spaces or end of line
        while i < len {
            if bytes[i] == b' ' {
                // Check if this is 2+ spaces (column separator)
                let space_start = i;
                while i < len && bytes[i] == b' ' {
                    i += 1;
                }
                if i - space_start >= 2 || i == len {
                    // Column separator found (or end of line)
                    let name = header[start..space_start].to_string();
                    matches.push((start, name));
                    break;
                }
                // Single space — part of column name (e.g. "IMAGE ID"), continue
            } else {
                i += 1;
            }
        }

        // Handle last column with no trailing spaces
        if i == len && start < len {
            let trailing = header[start..].trim_end().to_string();
            if !trailing.is_empty() && !matches.iter().any(|(s, _)| *s == start) {
                matches.push((start, trailing));
            }
        }
    }

    for (idx, (start, name)) in matches.iter().enumerate() {
        let end = if idx + 1 < matches.len() {
            matches[idx + 1].0
        } else {
            usize::MAX
        };
        cols.push(ColumnDef {
            name: name.clone(),
            start: *start,
            end,
        });
    }

    cols
}

/// Remove a column from a line by its index in col_positions.
fn strip_column(line: &str, cols: &[ColumnDef], strip_idx: Option<usize>) -> String {
    let strip_idx = match strip_idx {
        Some(idx) => idx,
        None => return line.to_string(),
    };

    if strip_idx >= cols.len() {
        return line.to_string();
    }

    let col = &cols[strip_idx];
    let line_len = line.len();

    if col.start >= line_len {
        return line.to_string();
    }

    let before = &line[..col.start];
    let after = if col.end < line_len {
        &line[col.end..]
    } else {
        ""
    };

    let combined = format!("{before}{after}");
    // Collapse excessive spaces but keep at least 3 between columns
    let collapse_re = Regex::new(r" {4,}").unwrap();
    collapse_re
        .replace_all(&combined, "   ")
        .trim_end()
        .to_string()
}

/// Strip timestamp prefix from a log line.
fn strip_timestamp(line: &str, re: &Regex) -> String {
    re.replace(line, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- docker ps tests --

    #[test]
    fn docker_ps_strips_ports_column() {
        let input = "\
CONTAINER ID   IMAGE          COMMAND       CREATED        STATUS        PORTS                  NAMES
abc123def456   nginx:latest   \"nginx -g\"    2 hours ago    Up 2 hours    0.0.0.0:80->80/tcp     web
def789abc012   redis:7        \"redis-ser\"   3 hours ago    Up 3 hours    0.0.0.0:6379->6379/tcp cache";

        let result = filter_docker_ps(input, 0);
        assert!(!result.contains("0.0.0.0:80"), "Should strip PORTS data");
        assert!(!result.contains("6379"), "Should strip PORTS data");
        assert!(result.contains("nginx:latest"));
        assert!(result.contains("web"));
        assert!(result.contains("redis:7"));
        assert!(result.contains("cache"));
        assert!(result.contains("NAMES"));
        assert!(!result.contains("PORTS"));
    }

    #[test]
    fn docker_ps_empty_output() {
        let input = "CONTAINER ID   IMAGE   COMMAND   CREATED   STATUS   PORTS   NAMES";
        let result = filter_docker_ps(input, 0);
        assert_eq!(result, "No containers.");
    }

    #[test]
    fn docker_ps_no_output() {
        let result = filter_docker_ps("", 0);
        assert_eq!(result, "No containers.");
    }

    #[test]
    fn docker_ps_preserves_status() {
        let input = "\
CONTAINER ID   IMAGE          COMMAND       CREATED        STATUS          PORTS     NAMES
abc123def456   myapp:v2       \"./start\"     5 min ago      Up 5 minutes    8080/tcp  app";

        let result = filter_docker_ps(input, 0);
        assert!(result.contains("Up 5 minutes"));
        assert!(result.contains("STATUS"));
    }

    // -- docker images tests --

    #[test]
    fn docker_images_strips_image_id() {
        let input = "\
REPOSITORY    TAG       IMAGE ID       CREATED        SIZE
nginx         latest    a8758716bb6a   2 weeks ago    187MB
redis         7         5f2e708d56aa   3 weeks ago    130MB
postgres      15        3b1a4a564f56   1 month ago    412MB";

        let result = filter_docker_images(input, 0);
        assert!(
            !result.contains("a8758716bb6a"),
            "Should strip IMAGE ID values"
        );
        assert!(!result.contains("5f2e708d56aa"));
        assert!(!result.contains("IMAGE ID"), "Should strip IMAGE ID header");
        assert!(result.contains("REPOSITORY"));
        assert!(result.contains("TAG"));
        assert!(result.contains("SIZE"));
        assert!(result.contains("nginx"));
        assert!(result.contains("187MB"));
    }

    #[test]
    fn docker_images_empty() {
        let input = "REPOSITORY   TAG   IMAGE ID   CREATED   SIZE";
        let result = filter_docker_images(input, 0);
        assert_eq!(result, "No images.");
    }

    #[test]
    fn docker_images_no_output() {
        let result = filter_docker_images("", 0);
        assert_eq!(result, "No images.");
    }

    #[test]
    fn docker_images_preserves_all_repos() {
        let input = "\
REPOSITORY      TAG       IMAGE ID       CREATED        SIZE
myapp           v1.2.3    abc123def456   1 day ago      95MB
myapp           latest    def456abc123   1 day ago      95MB";

        let result = filter_docker_images(input, 0);
        assert!(result.contains("myapp"));
        assert!(result.contains("v1.2.3"));
        assert!(result.contains("latest"));
        assert!(result.contains("95MB"));
        let line_count = result.lines().count();
        assert_eq!(line_count, 3, "Should have header + 2 data lines");
    }

    // -- docker logs tests --

    #[test]
    fn docker_logs_strips_timestamps() {
        let input = "\
2024-01-15T10:30:00.123Z Starting server...
2024-01-15T10:30:01.456Z Listening on port 8080
2024-01-15T10:30:02.789Z Ready to accept connections";

        let result = filter_docker_logs(input, 0);
        assert!(!result.contains("2024-01-15"), "Should strip timestamps");
        assert!(result.contains("Starting server..."));
        assert!(result.contains("Listening on port 8080"));
        assert!(result.contains("Ready to accept connections"));
    }

    #[test]
    fn docker_logs_truncates_long_output() {
        let mut lines = Vec::new();
        for i in 0..150 {
            lines.push(format!("2024-01-15T10:30:00Z Log line {i}"));
        }
        let input = lines.join("\n");

        let result = filter_docker_logs(&input, 0);
        assert!(result.contains("(150 total lines, showing last 50)"));
        assert!(result.contains("Log line 149"));
        assert!(result.contains("Log line 100"));
        assert!(
            !result.contains("Log line 99\n"),
            "Should not contain early lines"
        );
    }

    #[test]
    fn docker_logs_short_output_passes_through() {
        let input = "Server started\nConnection accepted\nRequest handled";

        let result = filter_docker_logs(input, 0);
        assert!(result.contains("Server started"));
        assert!(result.contains("Connection accepted"));
        assert!(result.contains("Request handled"));
        assert!(!result.contains("total lines"));
    }

    #[test]
    fn docker_logs_empty() {
        let result = filter_docker_logs("", 0);
        assert_eq!(result, "No log output.");
    }

    // -- docker compose tests --

    #[test]
    fn docker_compose_keeps_container_actions() {
        let input = "\
[+] Running 3/3
 ✔ Network myapp_default  Created
 ✔ Container myapp-db-1   Started
 ✔ Container myapp-web-1  Started
 ✔ Container myapp-redis-1 Started";

        let result = filter_docker_compose(input, 0);
        assert!(result.contains("Container myapp-db-1   Started"));
        assert!(result.contains("Container myapp-web-1  Started"));
        assert!(result.contains("Container myapp-redis-1 Started"));
    }

    #[test]
    fn docker_compose_drops_pull_progress() {
        let input = "\
[+] Pulling 3/3
 ⠋ nginx Pulling   12.3s
 ⠋ redis Pulling   8.1s
abc123: Pull complete
def456: Pull complete
Digest: sha256:abcdef123456
Status: Downloaded newer image for nginx:latest
 ✔ Container myapp-web-1 Started";

        let result = filter_docker_compose(input, 0);
        assert!(!result.contains("Pull complete"));
        assert!(!result.contains("Pulling"));
        assert!(!result.contains("Digest:"));
        assert!(result.contains("Container myapp-web-1 Started"));
    }

    #[test]
    fn docker_compose_drops_build_output() {
        let input = "\
[+] Building 2.1s (12/12) FINISHED
 => [internal] load build definition from Dockerfile
 => [internal] load .dockerignore
 => [1/5] FROM docker.io/library/node:18
 => [2/5] WORKDIR /app
 => [3/5] COPY package*.json ./
 => exporting to image
 ✔ Container myapp-web-1 Started
 ✔ Container myapp-db-1  Started";

        let result = filter_docker_compose(input, 0);
        assert!(!result.contains("load build definition"));
        assert!(!result.contains("WORKDIR"));
        assert!(!result.contains("COPY package"));
        assert!(result.contains("Container myapp-web-1 Started"));
        assert!(result.contains("Container myapp-db-1  Started"));
    }

    #[test]
    fn docker_compose_keeps_errors() {
        let input = "\
Error response from daemon: Conflict
error during connect: connection refused";

        let result = filter_docker_compose(input, 1);
        assert!(result.contains("Error response from daemon"));
        assert!(result.contains("error during connect"));
    }

    #[test]
    fn docker_compose_empty_success() {
        let result = filter_docker_compose("", 0);
        assert_eq!(result, "Docker compose completed.");
    }

    #[test]
    fn docker_compose_empty_failure() {
        let result = filter_docker_compose("", 1);
        assert_eq!(result, "Docker compose failed (exit code 1).");
    }

    // -- docker compose logs tests --

    #[test]
    fn compose_logs_strips_timestamps() {
        let input = "\
web-1  | 2024-01-15T10:30:00.123Z Starting server...
web-1  | 2024-01-15T10:30:01.456Z Listening on port 8080
db-1   | 2024-01-15T10:30:00.000Z PostgreSQL ready";

        let result = filter_docker_compose_logs(input, 0);
        assert!(!result.contains("2024-01-15"), "Should strip timestamps");
        assert!(result.contains("Starting server..."));
        assert!(result.contains("Listening on port 8080"));
        assert!(result.contains("PostgreSQL ready"));
    }

    #[test]
    fn compose_logs_dedupes_container_prefixes() {
        let input = "\
web-1  | Starting server...
web-1  | Listening on port 8080
web-1  | Ready to accept connections
db-1   | PostgreSQL starting
db-1   | PostgreSQL ready";

        let result = filter_docker_compose_logs(input, 0);
        // First occurrence of each container keeps prefix
        assert!(result.contains("web-1  | Starting server..."));
        assert!(result.contains("db-1   | PostgreSQL starting"));
        // Subsequent lines from same container omit prefix
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[1].trim(), "Listening on port 8080");
        assert_eq!(lines[2].trim(), "Ready to accept connections");
        assert_eq!(lines[4].trim(), "PostgreSQL ready");
    }

    #[test]
    fn compose_logs_truncates_long_output() {
        let mut lines = Vec::new();
        for i in 0..250 {
            lines.push(format!("web-1  | 2024-01-15T10:30:00Z Log line {i}"));
        }
        let input = lines.join("\n");

        let result = filter_docker_compose_logs(&input, 0);
        assert!(result.contains("...150 lines omitted..."));
        assert!(result.contains("Log line 0"));
        assert!(result.contains("Log line 249"));
    }

    #[test]
    fn compose_logs_empty() {
        let result = filter_docker_compose_logs("", 0);
        assert_eq!(result, "No log output.");
    }

    #[test]
    fn compose_logs_short_passthrough() {
        let input = "web-1  | Hello\ndb-1   | World";
        let result = filter_docker_compose_logs(input, 0);
        assert!(result.contains("web-1  | Hello"));
        assert!(result.contains("db-1   | World"));
        assert!(!result.contains("omitted"));
    }

    #[test]
    fn compose_logs_mixed_containers_no_false_dedup() {
        let input = "\
web-1  | Request 1
db-1   | Query 1
web-1  | Request 2";

        let result = filter_docker_compose_logs(input, 0);
        let lines: Vec<&str> = result.lines().collect();
        // web-1 appears, then db-1, then web-1 again — prefix should reappear
        assert!(lines[0].contains("web-1"));
        assert!(lines[1].contains("db-1"));
        assert!(lines[2].contains("web-1"));
    }
}
