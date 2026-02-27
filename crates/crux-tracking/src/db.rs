use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

/// Get the default database path (~/.local/share/crux/crux.db)
pub fn default_db_path() -> Result<PathBuf> {
    let data_dir = dirs_or_fallback();
    std::fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("crux.db"))
}

fn dirs_or_fallback() -> PathBuf {
    // Use XDG_DATA_HOME or fallback to ~/.local/share/crux
    std::env::var("XDG_DATA_HOME")
        .map(|d| PathBuf::from(d).join("crux"))
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".local/share/crux")
        })
}

/// Open or create the database, run migrations
pub fn open_db(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open an in-memory database (useful for testing)
pub fn open_memory_db() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS filter_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL DEFAULT (datetime('now')),
            command TEXT NOT NULL,
            filter_name TEXT,
            input_bytes INTEGER NOT NULL,
            output_bytes INTEGER NOT NULL,
            savings_bytes INTEGER NOT NULL,
            savings_pct REAL NOT NULL,
            exit_code INTEGER NOT NULL DEFAULT 0,
            duration_ms INTEGER
        );

        CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL DEFAULT (datetime('now')),
            command TEXT NOT NULL,
            raw_output TEXT NOT NULL,
            filtered_output TEXT NOT NULL,
            filter_name TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON filter_events(timestamp);
        CREATE INDEX IF NOT EXISTS idx_events_command ON filter_events(command);
        CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history(timestamp);
    ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_memory_db() {
        let conn = open_memory_db().expect("should open in-memory db");
        // Verify tables exist by querying them
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM filter_events", [], |row| row.get(0))
            .expect("filter_events table should exist");
        assert_eq!(count, 0);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .expect("history table should exist");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_migrate_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).expect("first migration should succeed");
        migrate(&conn).expect("second migration should also succeed");
    }

    #[test]
    fn test_dirs_or_fallback_default() {
        // Just verify it returns a path without panicking
        let path = dirs_or_fallback();
        assert!(path.to_str().is_some());
    }
}
