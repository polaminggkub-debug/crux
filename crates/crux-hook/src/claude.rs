use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct HookOutput {
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
}

/// Process a Claude Code PreToolUse hook call.
/// If the tool is "Bash" and the command could benefit from crux,
/// rewrite it to go through `crux run`.
pub fn handle_hook(input: &HookInput) -> Result<HookOutput> {
    if input.tool_name != "Bash" {
        return Ok(HookOutput {
            result: "approve".into(),
            tool_input: None,
        });
    }

    let command = input
        .tool_input
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if should_intercept(command) {
        let new_command = format!("crux run {command}");
        let mut new_input = input.tool_input.clone();
        new_input["command"] = serde_json::Value::String(new_command);
        Ok(HookOutput {
            result: "modify".into(),
            tool_input: Some(new_input),
        })
    } else {
        Ok(HookOutput {
            result: "approve".into(),
            tool_input: None,
        })
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

    #[test]
    fn non_bash_tool_approved() {
        let input = make_input("Read", "/some/file");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "approve");
        assert!(output.tool_input.is_none());
    }

    #[test]
    fn git_command_intercepted() {
        let input = make_input("Bash", "git status");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
        let new_cmd = output.tool_input.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(new_cmd, "crux run git status");
    }

    #[test]
    fn cargo_command_intercepted() {
        let input = make_input("Bash", "cargo test --release");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
        let new_cmd = output.tool_input.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(new_cmd, "crux run cargo test --release");
    }

    #[test]
    fn unknown_command_approved() {
        let input = make_input("Bash", "python script.py");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "approve");
        assert!(output.tool_input.is_none());
    }

    #[test]
    fn already_crux_not_double_wrapped() {
        let input = make_input("Bash", "crux run git status");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "approve");
        assert!(output.tool_input.is_none());
    }

    #[test]
    fn docker_command_intercepted() {
        let input = make_input("Bash", "docker ps");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn npm_command_intercepted() {
        let input = make_input("Bash", "npm test");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn empty_command_approved() {
        let input = make_input("Bash", "");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "approve");
        assert!(output.tool_input.is_none());
    }

    #[test]
    fn missing_command_field_approved() {
        let input = HookInput {
            tool_name: "Bash".to_string(),
            tool_input: json!({}),
        };
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "approve");
    }

    // -- Test runners --

    #[test]
    fn pytest_command_intercepted() {
        let input = make_input("Bash", "pytest --verbose");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
        let new_cmd = output.tool_input.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(new_cmd, "crux run pytest --verbose");
    }

    #[test]
    fn vitest_command_intercepted() {
        let input = make_input("Bash", "vitest run");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn jest_command_intercepted() {
        let input = make_input("Bash", "jest --coverage");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    // -- JS build tools --

    #[test]
    fn tsc_command_intercepted() {
        let input = make_input("Bash", "tsc --noEmit");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn eslint_command_intercepted() {
        let input = make_input("Bash", "eslint src/");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn prettier_command_intercepted() {
        let input = make_input("Bash", "prettier --check .");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn next_command_intercepted() {
        let input = make_input("Bash", "next build");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
        let new_cmd = output.tool_input.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(new_cmd, "crux run next build");
    }

    // -- Python tools --

    #[test]
    fn pip_command_intercepted() {
        let input = make_input("Bash", "pip install requests");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn ruff_command_intercepted() {
        let input = make_input("Bash", "ruff check src/");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
        let new_cmd = output.tool_input.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(new_cmd, "crux run ruff check src/");
    }

    // -- Go tools --

    #[test]
    fn golangci_lint_command_intercepted() {
        let input = make_input("Bash", "golangci-lint run");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    // -- Infrastructure & ops --

    #[test]
    fn terraform_command_intercepted() {
        let input = make_input("Bash", "terraform plan");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
        let new_cmd = output.tool_input.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(new_cmd, "crux run terraform plan");
    }

    #[test]
    fn helm_command_intercepted() {
        let input = make_input("Bash", "helm install my-release chart/");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn ansible_command_intercepted() {
        let input = make_input("Bash", "ansible playbook.yml");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn ssh_command_intercepted() {
        let input = make_input("Bash", "ssh user@host ls");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    // -- Build systems --

    #[test]
    fn make_command_intercepted() {
        let input = make_input("Bash", "make build");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
        let new_cmd = output.tool_input.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(new_cmd, "crux run make build");
    }

    #[test]
    fn mvn_command_intercepted() {
        let input = make_input("Bash", "mvn clean install");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn rustc_command_intercepted() {
        let input = make_input("Bash", "rustc --edition 2021 main.rs");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    // -- Filesystem & utilities --

    #[test]
    fn ls_command_intercepted() {
        let input = make_input("Bash", "ls -la");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn find_command_intercepted() {
        let input = make_input("Bash", "find . -name '*.rs'");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn grep_command_intercepted() {
        let input = make_input("Bash", "grep -r TODO src/");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn tree_command_intercepted() {
        let input = make_input("Bash", "tree -L 2");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn cat_command_intercepted() {
        let input = make_input("Bash", "cat README.md");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn curl_command_intercepted() {
        let input = make_input("Bash", "curl -s https://api.example.com");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn wget_command_intercepted() {
        let input = make_input("Bash", "wget https://example.com/file.tar.gz");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    #[test]
    fn wc_command_intercepted() {
        let input = make_input("Bash", "wc -l src/*.rs");
        let output = handle_hook(&input).unwrap();
        assert_eq!(output.result, "modify");
    }

    // -- Serialization --

    #[test]
    fn serialization_skip_none() {
        let output = HookOutput {
            result: "approve".into(),
            tool_input: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("tool_input"));
    }
}
