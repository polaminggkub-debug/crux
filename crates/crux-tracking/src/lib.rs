pub mod db;
pub mod events;
pub mod history;

// Re-export key types for convenience
pub use db::{default_db_path, open_db, open_memory_db};
pub use events::{get_gain_summary, record_event, FilterEvent, GainSummary};
pub use history::{get_recent_history, store_history, HistoryEntry};
