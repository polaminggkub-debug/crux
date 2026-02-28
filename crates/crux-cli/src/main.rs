mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::Read;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "crux", version, about = "CLI output compressor for AI agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command through the filter pipeline
    Run {
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
        /// Print execution timing breakdown to stderr
        #[arg(long)]
        time: bool,
    },
    /// Show token savings summary
    Gain {
        #[arg(long)]
        by_command: bool,
    },
    /// Show recent command history
    #[cfg(feature = "tracking")]
    History {
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Install Claude Code hook
    Init {
        #[arg(long, group = "target")]
        global: bool,
        #[arg(long, group = "target")]
        codex: bool,
    },
    /// List available filters
    Ls,
    /// Show which filter matches a command
    Which {
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// Show filter config details
    Show { filter: String },
    /// Export builtin filter as TOML for customization
    Eject { filter: String },
    /// Run declarative filter tests
    Verify,
    /// Keep only error/warning lines from command output
    Err {
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// Extract test summary from command output.
    ///
    /// Auto-detects: cargo test, pytest, jest, vitest, go test, mocha,
    /// playwright, rspec, PHPUnit, dotnet test. Falls back to extracting
    /// lines containing pass/fail/error/warning keywords.
    Test {
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// Run command with dedup and collapse filters
    Log {
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// Run diagnostic checks on your crux installation
    Doctor,
    /// Agent hook management
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
}

#[derive(Subcommand)]
enum HookCommand {
    /// Process Claude Code PreToolUse hook from stdin
    Handle,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Run { command, time } => cmd_run(&command, time),
        Commands::Gain { by_command } => cmd_gain(by_command),
        #[cfg(feature = "tracking")]
        Commands::History { limit } => cmd_history(limit),
        Commands::Init { global, codex } => commands::cmd_init(global, codex),
        Commands::Ls => commands::cmd_ls(),
        Commands::Which { command } => cmd_which(&command),
        Commands::Show { filter } => commands::cmd_show(&filter),
        Commands::Eject { filter } => commands::cmd_eject(&filter),
        Commands::Verify => commands::cmd_verify(),
        Commands::Err { command } => commands::cmd_err(&command),
        Commands::Test { command } => commands::cmd_test(&command),
        Commands::Log { command } => commands::cmd_log(&command),
        Commands::Doctor => commands::cmd_doctor(),
        Commands::Hook { command } => match command {
            HookCommand::Handle => cmd_hook_handle(),
        },
    };

    if let Err(e) = result {
        eprintln!("crux: error: {e:#}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

fn cmd_run(command: &[String], show_time: bool) -> Result<()> {
    let wall_start = Instant::now();

    let filter = crux_core::config::resolve_filter(command);

    let exec_start = Instant::now();
    let result = crux_core::runner::run_command(command)?;
    let exec_elapsed = exec_start.elapsed();

    let raw_output = &result.combined;
    let input_bytes = raw_output.len();

    let filter_start = Instant::now();
    let filtered = if let Some(ref config) = filter {
        crux_core::filter::apply_filter(config, raw_output, result.exit_code)
    } else {
        raw_output.clone()
    };
    let filter_elapsed = filter_start.elapsed();
    let output_bytes = filtered.len();

    print!("{filtered}");
    if !filtered.ends_with('\n') && !filtered.is_empty() {
        println!();
    }

    if result.exit_code != 0 {
        eprintln!("crux: exit code {}", result.exit_code);
    }

    #[cfg(feature = "tracking")]
    {
        let duration_ms = wall_start.elapsed().as_millis() as u64;
        if let Err(e) = record_tracking_and_history(
            command,
            &filter,
            input_bytes,
            output_bytes,
            result.exit_code,
            duration_ms,
            raw_output,
            &filtered,
        ) {
            eprintln!("crux: tracking error: {e}");
        }
    }

    #[cfg(not(feature = "tracking"))]
    let _ = wall_start;

    if input_bytes > 0 && input_bytes != output_bytes {
        let saved_pct = ((input_bytes - output_bytes) as f64 / input_bytes as f64) * 100.0;
        eprintln!("crux: {input_bytes} → {output_bytes} bytes ({saved_pct:.0}% saved)");
    }

    if show_time {
        let wall_elapsed = wall_start.elapsed();
        eprintln!("crux: timing breakdown:");
        eprintln!(
            "  command execution: {:.3}ms",
            exec_elapsed.as_secs_f64() * 1000.0
        );
        eprintln!(
            "  filter pipeline:  {:.3}ms",
            filter_elapsed.as_secs_f64() * 1000.0
        );
        eprintln!(
            "  total wall time:  {:.3}ms",
            wall_elapsed.as_secs_f64() * 1000.0
        );
        eprintln!("  input size:       {} bytes", input_bytes);
        eprintln!("  output size:      {} bytes", output_bytes);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tracking helpers
// ---------------------------------------------------------------------------

#[cfg(feature = "tracking")]
#[allow(clippy::too_many_arguments)]
fn record_tracking_and_history(
    command: &[String],
    filter: &Option<crux_core::config::FilterConfig>,
    input_bytes: usize,
    output_bytes: usize,
    exit_code: i32,
    duration_ms: u64,
    raw_output: &str,
    filtered_output: &str,
) -> Result<()> {
    let db_path = crux_tracking::db::default_db_path()?;
    let conn = crux_tracking::db::open_db(&db_path)?;
    let cmd_str = command.join(" ");
    let filter_name = filter.as_ref().map(|f| f.command.clone());

    let event = crux_tracking::events::FilterEvent {
        command: cmd_str.clone(),
        filter_name: filter_name.clone(),
        input_bytes,
        output_bytes,
        exit_code,
        duration_ms: Some(duration_ms),
    };
    crux_tracking::events::record_event(&conn, &event)?;

    crux_tracking::history::store_history(
        &conn,
        &cmd_str,
        raw_output,
        filtered_output,
        filter_name.as_deref(),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Gain
// ---------------------------------------------------------------------------

fn cmd_gain(by_command: bool) -> Result<()> {
    #[cfg(feature = "tracking")]
    {
        let db_path = crux_tracking::db::default_db_path()?;
        let conn = crux_tracking::db::open_db(&db_path)?;

        if by_command {
            let summaries = crux_tracking::events::get_per_command_summary(&conn)?;
            if summaries.is_empty() {
                println!("No filter events recorded yet. Run some commands through crux first!");
                return Ok(());
            }
            println!(
                "{:<30} {:>5} {:>12} {:>12} {:>6}",
                "COMMAND", "RUNS", "INPUT", "SAVED", "AVG%"
            );
            println!("{}", "─".repeat(69));
            for s in &summaries {
                println!(
                    "{:<30} {:>5} {:>10} B {:>10} B {:>5.1}%",
                    truncate_str(&s.command, 30),
                    s.events,
                    s.total_input_bytes,
                    s.total_savings_bytes,
                    s.avg_savings_pct,
                );
            }
        } else {
            let summary = crux_tracking::events::get_gain_summary(&conn)?;
            if summary.total_events == 0 {
                println!("No filter events recorded yet. Run some commands through crux first!");
                return Ok(());
            }
            println!("crux token savings summary");
            println!("──────────────────────────");
            println!("Total events:  {}", summary.total_events);
            println!("Total input:   {} bytes", summary.total_input_bytes);
            println!("Total output:  {} bytes", summary.total_output_bytes);
            println!("Total saved:   {} bytes", summary.total_savings_bytes);
            println!("Avg savings:   {:.1}%", summary.avg_savings_pct);
        }
        Ok(())
    }

    #[cfg(not(feature = "tracking"))]
    {
        let _ = by_command;
        eprintln!("crux: tracking feature is not enabled");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[cfg(feature = "tracking")]
fn cmd_history(limit: usize) -> Result<()> {
    let db_path = crux_tracking::db::default_db_path()?;
    let conn = crux_tracking::db::open_db(&db_path)?;
    let entries = crux_tracking::history::get_recent_history(&conn, limit)?;

    if entries.is_empty() {
        println!("No history entries yet. Run some commands through crux first!");
        return Ok(());
    }

    for entry in &entries {
        let raw_len = entry.raw_output.len();
        let filtered_len = entry.filtered_output.len();
        let savings_pct = if raw_len > 0 {
            ((raw_len - filtered_len) as f64 / raw_len as f64) * 100.0
        } else {
            0.0
        };
        let filter_label = entry.filter_name.as_deref().unwrap_or("(passthrough)");
        println!(
            "[{}] {} | filter: {} | {:.0}% saved",
            entry.timestamp, entry.command, filter_label, savings_pct
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Which
// ---------------------------------------------------------------------------

fn cmd_which(command: &[String]) -> Result<()> {
    match crux_core::config::resolve_filter(command) {
        Some(config) => {
            println!("Filter:      {}", config.command);
            if let Some(desc) = &config.description {
                println!("Description: {desc}");
            }
            println!("Priority:    {}", config.priority);
        }
        None => {
            println!("No filter matches: {}", command.join(" "));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

// ---------------------------------------------------------------------------
// Hook handle — process Claude Code PreToolUse hook from stdin
// ---------------------------------------------------------------------------

fn cmd_hook_handle() -> Result<()> {
    // Read all of stdin
    let mut input_str = String::new();
    std::io::stdin().read_to_string(&mut input_str).ok();

    // Parse and handle — silent on any error (never block Claude Code)
    if let Ok(input) = serde_json::from_str::<crux_hook::claude::HookInput>(&input_str) {
        if let Some(output) = crux_hook::claude::handle_hook(&input) {
            if let Ok(json) = serde_json::to_string(&output) {
                println!("{json}");
            }
        }
    }

    Ok(())
}
