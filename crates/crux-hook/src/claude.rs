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
        "git ", "cargo ", "npm ", "npx ", "pnpm ", "yarn ", "docker ", "go ", "kubectl ",
        "gradle ", "gh ",
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
