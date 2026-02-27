use anyhow::Result;
use rusqlite::Connection;

/// A filter event to record in the database.
pub struct FilterEvent {
    pub command: String,
    pub filter_name: Option<String>,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub exit_code: i32,
    pub duration_ms: Option<u64>,
}

/// Record a filter event (input/output sizes, savings, etc.)
pub fn record_event(conn: &Connection, event: &FilterEvent) -> Result<()> {
    let savings = event.input_bytes as i64 - event.output_bytes as i64;
    let pct = if event.input_bytes > 0 {
        (savings as f64 / event.input_bytes as f64) * 100.0
    } else {
        0.0
    };

    conn.execute(
        "INSERT INTO filter_events (command, filter_name, input_bytes, output_bytes, savings_bytes, savings_pct, exit_code, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            event.command,
            event.filter_name,
            event.input_bytes as i64,
            event.output_bytes as i64,
            savings,
            pct,
            event.exit_code,
            event.duration_ms.map(|d| d as i64),
        ],
    )?;
    Ok(())
}

/// Aggregate savings summary across all recorded events.
pub struct GainSummary {
    pub total_input_bytes: i64,
    pub total_output_bytes: i64,
    pub total_savings_bytes: i64,
    pub avg_savings_pct: f64,
    pub total_events: i64,
}

/// Get total savings summary across all recorded filter events.
pub fn get_gain_summary(conn: &Connection) -> Result<GainSummary> {
    let summary = conn.query_row(
        "SELECT
            COALESCE(SUM(input_bytes), 0),
            COALESCE(SUM(output_bytes), 0),
            COALESCE(SUM(savings_bytes), 0),
            COALESCE(AVG(savings_pct), 0.0),
            COUNT(*)
         FROM filter_events",
        [],
        |row| {
            Ok(GainSummary {
                total_input_bytes: row.get(0)?,
                total_output_bytes: row.get(1)?,
                total_savings_bytes: row.get(2)?,
                avg_savings_pct: row.get(3)?,
                total_events: row.get(4)?,
            })
        },
    )?;
    Ok(summary)
}

/// Per-command savings breakdown.
pub struct CommandSummary {
    pub command: String,
    pub events: i64,
    pub total_input_bytes: i64,
    pub total_output_bytes: i64,
    pub total_savings_bytes: i64,
    pub avg_savings_pct: f64,
}

/// Get savings summary grouped by command, ordered by total savings descending.
pub fn get_per_command_summary(conn: &Connection) -> Result<Vec<CommandSummary>> {
    let mut stmt = conn.prepare(
        "SELECT
            command,
            COUNT(*) as events,
            COALESCE(SUM(input_bytes), 0),
            COALESCE(SUM(output_bytes), 0),
            COALESCE(SUM(savings_bytes), 0),
            COALESCE(AVG(savings_pct), 0.0)
         FROM filter_events
         GROUP BY command
         ORDER BY SUM(savings_bytes) DESC",
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CommandSummary {
                command: row.get(0)?,
                events: row.get(1)?,
                total_input_bytes: row.get(2)?,
                total_output_bytes: row.get(3)?,
                total_savings_bytes: row.get(4)?,
                avg_savings_pct: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory_db;

    #[test]
    fn test_record_event() {
        let conn = open_memory_db().unwrap();
        let event = FilterEvent {
            command: "cargo test".to_string(),
            filter_name: Some("cargo-test".to_string()),
            input_bytes: 1000,
            output_bytes: 300,
            exit_code: 0,
            duration_ms: Some(150),
        };
        record_event(&conn, &event).expect("should record event");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM filter_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_record_event_no_filter_name() {
        let conn = open_memory_db().unwrap();
        let event = FilterEvent {
            command: "ls -la".to_string(),
            filter_name: None,
            input_bytes: 500,
            output_bytes: 500,
            exit_code: 0,
            duration_ms: None,
        };
        record_event(&conn, &event).expect("should record event without filter name");
    }

    #[test]
    fn test_savings_calculation() {
        let conn = open_memory_db().unwrap();
        let event = FilterEvent {
            command: "cargo test".to_string(),
            filter_name: Some("cargo-test".to_string()),
            input_bytes: 1000,
            output_bytes: 300,
            exit_code: 0,
            duration_ms: None,
        };
        record_event(&conn, &event).unwrap();

        let row: (i64, f64) = conn
            .query_row(
                "SELECT savings_bytes, savings_pct FROM filter_events WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(row.0, 700); // 1000 - 300
        assert!((row.1 - 70.0).abs() < 0.01); // 70%
    }

    #[test]
    fn test_zero_input_bytes() {
        let conn = open_memory_db().unwrap();
        let event = FilterEvent {
            command: "echo".to_string(),
            filter_name: None,
            input_bytes: 0,
            output_bytes: 0,
            exit_code: 0,
            duration_ms: None,
        };
        record_event(&conn, &event).unwrap();

        let pct: f64 = conn
            .query_row(
                "SELECT savings_pct FROM filter_events WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!((pct - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_gain_summary_empty() {
        let conn = open_memory_db().unwrap();
        let summary = get_gain_summary(&conn).unwrap();
        assert_eq!(summary.total_events, 0);
        assert_eq!(summary.total_input_bytes, 0);
        assert_eq!(summary.total_output_bytes, 0);
        assert_eq!(summary.total_savings_bytes, 0);
        assert!((summary.avg_savings_pct - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_per_command_summary_empty() {
        let conn = open_memory_db().unwrap();
        let summaries = get_per_command_summary(&conn).unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn test_per_command_summary_groups_by_command() {
        let conn = open_memory_db().unwrap();

        let events = vec![
            FilterEvent {
                command: "cargo test".to_string(),
                filter_name: Some("cargo-test".to_string()),
                input_bytes: 1000,
                output_bytes: 300,
                exit_code: 0,
                duration_ms: None,
            },
            FilterEvent {
                command: "cargo test".to_string(),
                filter_name: Some("cargo-test".to_string()),
                input_bytes: 2000,
                output_bytes: 600,
                exit_code: 0,
                duration_ms: None,
            },
            FilterEvent {
                command: "git status".to_string(),
                filter_name: Some("git-status".to_string()),
                input_bytes: 500,
                output_bytes: 100,
                exit_code: 0,
                duration_ms: None,
            },
        ];

        for e in &events {
            record_event(&conn, e).unwrap();
        }

        let summaries = get_per_command_summary(&conn).unwrap();
        assert_eq!(summaries.len(), 2);

        // Ordered by total savings DESC: cargo test saved 2100, git status saved 400
        assert_eq!(summaries[0].command, "cargo test");
        assert_eq!(summaries[0].events, 2);
        assert_eq!(summaries[0].total_input_bytes, 3000);
        assert_eq!(summaries[0].total_output_bytes, 900);
        assert_eq!(summaries[0].total_savings_bytes, 2100);

        assert_eq!(summaries[1].command, "git status");
        assert_eq!(summaries[1].events, 1);
        assert_eq!(summaries[1].total_savings_bytes, 400);
    }

    #[test]
    fn test_gain_summary_multiple_events() {
        let conn = open_memory_db().unwrap();

        let events = vec![
            FilterEvent {
                command: "cargo test".to_string(),
                filter_name: Some("cargo-test".to_string()),
                input_bytes: 1000,
                output_bytes: 300,
                exit_code: 0,
                duration_ms: Some(100),
            },
            FilterEvent {
                command: "cargo build".to_string(),
                filter_name: Some("cargo-build".to_string()),
                input_bytes: 2000,
                output_bytes: 500,
                exit_code: 0,
                duration_ms: Some(200),
            },
        ];

        for e in &events {
            record_event(&conn, e).unwrap();
        }

        let summary = get_gain_summary(&conn).unwrap();
        assert_eq!(summary.total_events, 2);
        assert_eq!(summary.total_input_bytes, 3000);
        assert_eq!(summary.total_output_bytes, 800);
        assert_eq!(summary.total_savings_bytes, 2200);
        // Event 1: 70%, Event 2: 75%, avg = 72.5%
        assert!((summary.avg_savings_pct - 72.5).abs() < 0.01);
    }
}
