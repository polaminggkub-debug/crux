use serde::{Deserialize, Serialize};

/// Input from Claude Code's PreToolUse hook (stdin JSON).
/// Extra fields like `session_id`, `hook_event_name` are ignored.
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: serde_json::Value,
}

/// Output to Claude Code (stdout JSON) — only emitted when rewriting.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    pub hook_specific_output: HookSpecificOutput,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSpecificOutput {
    pub hook_event_name: String,
    pub permission_decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
}

/// Process a Claude Code PreToolUse hook call.
///
/// Returns `None` for passthrough (caller prints nothing, exits 0).
/// Returns `Some(HookOutput)` when rewriting the command through crux.
pub fn handle_hook(input: &HookInput) -> Option<HookOutput> {
    if input.tool_name != "Bash" {
        return None;
    }

    let command = input
        .tool_input
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if let Some(rewritten) = rewrite_command(command) {
        let mut new_input = input.tool_input.clone();
        new_input["command"] = serde_json::Value::String(rewritten);

        Some(HookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".into(),
                permission_decision: "allow".into(),
                updated_input: Some(new_input),
            },
        })
    } else {
        None
    }
}

/// Attempt to rewrite a command string for crux filtering.
///
/// Handles:
/// - Simple commands: `git status` → `crux run git status`
/// - Compound commands: `cd /path && git status` → `cd /path && crux run git status`
/// - Chained commands: `cd /p && cargo test && echo done` → rewrites each eligible part
fn rewrite_command(command: &str) -> Option<String> {
    // Simple case: entire command is interceptable
    if should_intercept(command) {
        return Some(format!("crux run {command}"));
    }

    // Compound commands: split on && and ; operators, rewrite eligible parts
    // Only attempt if the command contains shell operators
    if !command.contains("&&") && !command.contains(';') {
        return None;
    }

    let mut result = String::new();
    let mut changed = false;
    let mut remaining = command;

    while !remaining.is_empty() {
        // Find next separator (&& or ;)
        let (sep, sep_pos) = find_next_separator(remaining);

        let (part, rest) = if let Some(pos) = sep_pos {
            let sep_str = sep.unwrap();
            (&remaining[..pos], &remaining[pos + sep_str.len()..])
        } else {
            (remaining, "")
        };

        let trimmed = part.trim();
        if should_intercept(trimmed) {
            result.push_str(&part.replace(trimmed, &format!("crux run {trimmed}")));
            changed = true;
        } else {
            result.push_str(part);
        }

        if let Some(sep_str) = sep {
            result.push_str(sep_str);
        }

        remaining = rest;
    }

    if changed {
        Some(result)
    } else {
        None
    }
}

/// Find the next `&&` or `;` separator in a command string.
/// Returns the separator string and its position.
fn find_next_separator(s: &str) -> (Option<&'static str>, Option<usize>) {
    let amp = s.find("&&");
    let semi = s.find(';');

    match (amp, semi) {
        (Some(a), Some(b)) if a <= b => (Some("&&"), Some(a)),
        (Some(_), Some(b)) => (Some(";"), Some(b)),
        (Some(a), None) => (Some("&&"), Some(a)),
        (None, Some(b)) => (Some(";"), Some(b)),
        (None, None) => (None, None),
    }
}

/// Check if a command should be intercepted by crux.
fn should_intercept(command: &str) -> bool {
    // Don't intercept if already going through crux
    if command.starts_with("crux ") {
        return false;
    }

    let known_prefixes = [
        // Version control
        "git ",
        "gh ",
        // Rust
        "cargo ",
        "rustc ",
        // JavaScript / Node
        "npm ",
        "npx ",
        "pnpm ",
        "yarn ",
        "next ",
        "tsc ",
        "eslint ",
        "prettier ",
        "vitest ",
        "jest ",
        "playwright ",
        // PHP / Laravel
        "php ",
        "composer ",
        "phpunit ",
        "pest ",
        // Python
        "pytest ",
        "pip ",
        "ruff ",
        // Go
        "go ",
        "golangci-lint ",
        // Java / JVM
        "gradle ",
        "mvn ",
        // Containers & orchestration
        "docker ",
        "kubectl ",
        "helm ",
        // Infrastructure & ops
        "terraform ",
        "ansible ",
        "ssh ",
        // Build systems
        "make ",
        // Filesystem & utilities
        "ls ",
        "find ",
        "grep ",
        "tree ",
        "cat ",
        "curl ",
        "wget ",
        "wc ",
    ];
    known_prefixes.iter().any(|p| command.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_input(tool_name: &str, command: &str) -> HookInput {
        HookInput {
            tool_name: tool_name.to_string(),
            tool_input: json!({ "command": command }),
        }
    }

    /// Helper: assert the hook returns Some with the expected rewritten command.
    fn assert_rewritten(input: &HookInput, expected_cmd: &str) {
        let output = handle_hook(input).expect("expected Some(HookOutput)");
        assert_eq!(output.hook_specific_output.hook_event_name, "PreToolUse");
        assert_eq!(output.hook_specific_output.permission_decision, "allow");
        let cmd = output.hook_specific_output.updated_input.as_ref().unwrap()["command"]
            .as_str()
            .unwrap();
        assert_eq!(cmd, expected_cmd);
    }

    /// Helper: assert the hook returns None (silent passthrough).
    fn assert_passthrough(input: &HookInput) {
        assert!(
            handle_hook(input).is_none(),
            "expected None (passthrough), got Some"
        );
    }

    // -- Passthrough cases --

    #[test]
    fn non_bash_tool_passthrough() {
        let input = make_input("Read", "/some/file");
        assert_passthrough(&input);
    }

    #[test]
    fn unknown_command_passthrough() {
        let input = make_input("Bash", "python script.py");
        assert_passthrough(&input);
    }

    #[test]
    fn already_crux_passthrough() {
        let input = make_input("Bash", "crux run git status");
        assert_passthrough(&input);
    }

    #[test]
    fn empty_command_passthrough() {
        let input = make_input("Bash", "");
        assert_passthrough(&input);
    }

    #[test]
    fn missing_command_field_passthrough() {
        let input = HookInput {
            tool_name: "Bash".to_string(),
            tool_input: json!({}),
        };
        assert_passthrough(&input);
    }

    // -- Deserialization of full Claude Code input --

    #[test]
    fn deserialize_full_claude_input() {
        let raw = r#"{"session_id":"abc","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"git status"}}"#;
        let input: HookInput = serde_json::from_str(raw).unwrap();
        assert_eq!(input.tool_name, "Bash");
        assert_eq!(input.tool_input["command"], "git status");
    }

    // -- Rewrite cases --

    #[test]
    fn git_command_rewritten() {
        let input = make_input("Bash", "git status");
        assert_rewritten(&input, "crux run git status");
    }

    #[test]
    fn cargo_command_rewritten() {
        let input = make_input("Bash", "cargo test --release");
        assert_rewritten(&input, "crux run cargo test --release");
    }

    #[test]
    fn docker_command_rewritten() {
        let input = make_input("Bash", "docker ps");
        assert_rewritten(&input, "crux run docker ps");
    }

    #[test]
    fn npm_command_rewritten() {
        let input = make_input("Bash", "npm test");
        assert_rewritten(&input, "crux run npm test");
    }

    // -- Test runners --

    #[test]
    fn pytest_command_rewritten() {
        let input = make_input("Bash", "pytest --verbose");
        assert_rewritten(&input, "crux run pytest --verbose");
    }

    #[test]
    fn vitest_command_rewritten() {
        let input = make_input("Bash", "vitest run");
        assert_rewritten(&input, "crux run vitest run");
    }

    #[test]
    fn jest_command_rewritten() {
        let input = make_input("Bash", "jest --coverage");
        assert_rewritten(&input, "crux run jest --coverage");
    }

    // -- JS build tools --

    #[test]
    fn tsc_command_rewritten() {
        let input = make_input("Bash", "tsc --noEmit");
        assert_rewritten(&input, "crux run tsc --noEmit");
    }

    #[test]
    fn eslint_command_rewritten() {
        let input = make_input("Bash", "eslint src/");
        assert_rewritten(&input, "crux run eslint src/");
    }

    #[test]
    fn prettier_command_rewritten() {
        let input = make_input("Bash", "prettier --check .");
        assert_rewritten(&input, "crux run prettier --check .");
    }

    #[test]
    fn next_command_rewritten() {
        let input = make_input("Bash", "next build");
        assert_rewritten(&input, "crux run next build");
    }

    // -- Python tools --

    #[test]
    fn pip_command_rewritten() {
        let input = make_input("Bash", "pip install requests");
        assert_rewritten(&input, "crux run pip install requests");
    }

    #[test]
    fn ruff_command_rewritten() {
        let input = make_input("Bash", "ruff check src/");
        assert_rewritten(&input, "crux run ruff check src/");
    }

    // -- Go tools --

    #[test]
    fn golangci_lint_command_rewritten() {
        let input = make_input("Bash", "golangci-lint run");
        assert_rewritten(&input, "crux run golangci-lint run");
    }

    // -- Infrastructure & ops --

    #[test]
    fn terraform_command_rewritten() {
        let input = make_input("Bash", "terraform plan");
        assert_rewritten(&input, "crux run terraform plan");
    }

    #[test]
    fn helm_command_rewritten() {
        let input = make_input("Bash", "helm install my-release chart/");
        assert_rewritten(&input, "crux run helm install my-release chart/");
    }

    #[test]
    fn ansible_command_rewritten() {
        let input = make_input("Bash", "ansible playbook.yml");
        assert_rewritten(&input, "crux run ansible playbook.yml");
    }

    #[test]
    fn ssh_command_rewritten() {
        let input = make_input("Bash", "ssh user@host ls");
        assert_rewritten(&input, "crux run ssh user@host ls");
    }

    // -- Build systems --

    #[test]
    fn make_command_rewritten() {
        let input = make_input("Bash", "make build");
        assert_rewritten(&input, "crux run make build");
    }

    #[test]
    fn mvn_command_rewritten() {
        let input = make_input("Bash", "mvn clean install");
        assert_rewritten(&input, "crux run mvn clean install");
    }

    #[test]
    fn rustc_command_rewritten() {
        let input = make_input("Bash", "rustc --edition 2021 main.rs");
        assert_rewritten(&input, "crux run rustc --edition 2021 main.rs");
    }

    // -- Filesystem & utilities --

    #[test]
    fn ls_command_rewritten() {
        let input = make_input("Bash", "ls -la");
        assert_rewritten(&input, "crux run ls -la");
    }

    #[test]
    fn find_command_rewritten() {
        let input = make_input("Bash", "find . -name '*.rs'");
        assert_rewritten(&input, "crux run find . -name '*.rs'");
    }

    #[test]
    fn grep_command_rewritten() {
        let input = make_input("Bash", "grep -r TODO src/");
        assert_rewritten(&input, "crux run grep -r TODO src/");
    }

    #[test]
    fn tree_command_rewritten() {
        let input = make_input("Bash", "tree -L 2");
        assert_rewritten(&input, "crux run tree -L 2");
    }

    #[test]
    fn cat_command_rewritten() {
        let input = make_input("Bash", "cat README.md");
        assert_rewritten(&input, "crux run cat README.md");
    }

    #[test]
    fn curl_command_rewritten() {
        let input = make_input("Bash", "curl -s https://api.example.com");
        assert_rewritten(&input, "crux run curl -s https://api.example.com");
    }

    #[test]
    fn wget_command_rewritten() {
        let input = make_input("Bash", "wget https://example.com/file.tar.gz");
        assert_rewritten(&input, "crux run wget https://example.com/file.tar.gz");
    }

    #[test]
    fn wc_command_rewritten() {
        let input = make_input("Bash", "wc -l src/*.rs");
        assert_rewritten(&input, "crux run wc -l src/*.rs");
    }

    // -- Compound commands --

    #[test]
    fn cd_then_git_rewritten() {
        let input = make_input("Bash", "cd /some/path && git status");
        assert_rewritten(&input, "cd /some/path && crux run git status");
    }

    #[test]
    fn cd_then_cargo_test_rewritten() {
        let input = make_input("Bash", "cd /project && cargo test --release");
        assert_rewritten(&input, "cd /project && crux run cargo test --release");
    }

    #[test]
    fn cd_then_npm_test_rewritten() {
        let input = make_input("Bash", "cd /app && npm test");
        assert_rewritten(&input, "cd /app && crux run npm test");
    }

    #[test]
    fn cd_then_unknown_passthrough() {
        let input = make_input("Bash", "cd /path && python script.py");
        assert_passthrough(&input);
    }

    #[test]
    fn multiple_eligible_commands_rewritten() {
        let input = make_input("Bash", "cd /p && cargo test && git status");
        assert_rewritten(
            &input,
            "cd /p && crux run cargo test && crux run git status",
        );
    }

    #[test]
    fn semicolon_compound_rewritten() {
        let input = make_input("Bash", "cd /p; git log --oneline");
        assert_rewritten(&input, "cd /p; crux run git log --oneline");
    }

    // -- Serialization format --

    #[test]
    fn output_serializes_to_correct_format() {
        let output = HookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".into(),
                permission_decision: "allow".into(),
                updated_input: Some(json!({ "command": "crux run git status" })),
            },
        };
        let json: serde_json::Value = serde_json::to_value(&output).unwrap();
        assert_eq!(json["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(json["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["command"],
            "crux run git status"
        );
    }

    #[test]
    fn output_skips_updated_input_when_none() {
        let output = HookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".into(),
                permission_decision: "allow".into(),
                updated_input: None,
            },
        };
        let json_str = serde_json::to_string(&output).unwrap();
        assert!(!json_str.contains("updatedInput"));
    }
}
