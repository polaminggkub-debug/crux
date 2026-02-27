use anyhow::Result;
use std::process::{Command, Stdio};

/// Result of running a command
#[derive(Debug)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    /// Combined output (stdout + stderr interleaved isn't possible, so concat)
    pub combined: String,
}

/// Execute a command and capture its output
pub fn run_command(args: &[String]) -> Result<CommandResult> {
    anyhow::ensure!(!args.is_empty(), "No command provided");

    let output = Command::new(&args[0])
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = if stderr.is_empty() {
        stdout.clone()
    } else if stdout.is_empty() {
        stderr.clone()
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    Ok(CommandResult {
        stdout,
        stderr,
        exit_code: output.status.code().unwrap_or(-1),
        combined,
    })
}

/// Compute baseline: how many bytes/chars the raw output is
pub fn baseline_size(result: &CommandResult) -> usize {
    result.combined.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_echo_hello() {
        let args: Vec<String> = vec!["echo".into(), "hello".into()];
        let result = run_command(&args).expect("echo should succeed");
        assert_eq!(result.stdout.trim(), "hello");
        assert_eq!(result.exit_code, 0);
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_nonexistent_command() {
        let args: Vec<String> = vec!["this-command-does-not-exist-xyz".into()];
        let result = run_command(&args);
        assert!(result.is_err(), "nonexistent command should return error");
    }

    #[test]
    fn test_exit_code_capture() {
        let args: Vec<String> = vec!["false".into()];
        let result = run_command(&args).expect("false should execute successfully");
        assert_ne!(
            result.exit_code, 0,
            "false command should have non-zero exit code"
        );
    }

    #[test]
    fn test_empty_args() {
        let args: Vec<String> = vec![];
        let result = run_command(&args);
        assert!(result.is_err(), "empty args should return error");
    }

    #[test]
    fn test_baseline_size() {
        let result = CommandResult {
            stdout: "hello".into(),
            stderr: String::new(),
            exit_code: 0,
            combined: "hello".into(),
        };
        assert_eq!(baseline_size(&result), 5);
    }

    #[test]
    fn test_combined_output() {
        // When both stdout and stderr have content, combined should concat them
        let args: Vec<String> = vec!["sh".into(), "-c".into(), "echo out; echo err >&2".into()];
        let result = run_command(&args).expect("sh should succeed");
        assert_eq!(result.stdout.trim(), "out");
        assert_eq!(result.stderr.trim(), "err");
        assert!(result.combined.contains("out"));
        assert!(result.combined.contains("err"));
    }
}
