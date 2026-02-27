use anyhow::Result;
use rusqlite::Connection;

/// A single history entry representing a filtered command output.
pub struct HistoryEntry {
    pub id: i64,
    pub timestamp: String,
    pub command: String,
    pub raw_output: String,
    pub filtered_output: String,
    pub filter_name: Option<String>,
}

/// Store a command's raw and filtered output in history.
pub fn store_history(
    conn: &Connection,
    command: &str,
    raw: &str,
    filtered: &str,
    filter_name: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO history (command, raw_output, filtered_output, filter_name)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![command, raw, filtered, filter_name],
    )?;
    Ok(())
}

/// Get the most recent history entries, ordered newest first.
pub fn get_recent_history(conn: &Connection, limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, command, raw_output, filtered_output, filter_name
         FROM history
         ORDER BY timestamp DESC
         LIMIT ?1",
    )?;

    let entries = stmt
        .query_map(rusqlite::params![limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                command: row.get(2)?,
                raw_output: row.get(3)?,
                filtered_output: row.get(4)?,
                filter_name: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory_db;

    #[test]
    fn test_store_history() {
        let conn = open_memory_db().unwrap();
        store_history(
            &conn,
            "cargo test",
            "raw output here",
            "filtered output here",
            Some("cargo-test"),
        )
        .expect("should store history");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_store_history_no_filter() {
        let conn = open_memory_db().unwrap();
        store_history(&conn, "ls -la", "file list", "file list", None)
            .expect("should store history without filter name");

        let entry: (String, Option<String>) = conn
            .query_row(
                "SELECT command, filter_name FROM history WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(entry.0, "ls -la");
        assert!(entry.1.is_none());
    }

    #[test]
    fn test_get_recent_history_empty() {
        let conn = open_memory_db().unwrap();
        let entries = get_recent_history(&conn, 10).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_get_recent_history_ordering() {
        let conn = open_memory_db().unwrap();

        // Insert multiple entries
        store_history(&conn, "cmd1", "raw1", "filtered1", Some("f1")).unwrap();
        store_history(&conn, "cmd2", "raw2", "filtered2", Some("f2")).unwrap();
        store_history(&conn, "cmd3", "raw3", "filtered3", None).unwrap();

        let entries = get_recent_history(&conn, 10).unwrap();
        assert_eq!(entries.len(), 3);
        // Most recent first (highest id, since timestamps may be identical in fast tests)
        assert_eq!(entries[0].command, "cmd3");
        assert_eq!(entries[1].command, "cmd2");
        assert_eq!(entries[2].command, "cmd1");
    }

    #[test]
    fn test_get_recent_history_limit() {
        let conn = open_memory_db().unwrap();

        for i in 0..5 {
            store_history(
                &conn,
                &format!("cmd{i}"),
                &format!("raw{i}"),
                &format!("filtered{i}"),
                None,
            )
            .unwrap();
        }

        let entries = get_recent_history(&conn, 2).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_history_entry_fields() {
        let conn = open_memory_db().unwrap();
        store_history(
            &conn,
            "cargo build",
            "compiling...\nfinished",
            "finished",
            Some("cargo-build"),
        )
        .unwrap();

        let entries = get_recent_history(&conn, 1).unwrap();
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];
        assert_eq!(entry.command, "cargo build");
        assert_eq!(entry.raw_output, "compiling...\nfinished");
        assert_eq!(entry.filtered_output, "finished");
        assert_eq!(entry.filter_name.as_deref(), Some("cargo-build"));
        assert!(!entry.timestamp.is_empty());
        assert!(entry.id > 0);
    }
}
