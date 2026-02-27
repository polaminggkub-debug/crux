#[cfg(feature = "lua")]
use mlua::prelude::*;

/// Apply a Lua filter to the output. Returns Some(filtered) if Lua returns a string, None for passthrough.
#[cfg(feature = "lua")]
pub fn apply_lua(source: &str, output: &str, exit_code: i32, args: &[String]) -> Option<String> {
    let lua = Lua::new();

    // Sandbox: remove dangerous globals
    if let Err(e) = lua.globals().set("os", mlua::Value::Nil) {
        eprintln!("crux: lua sandbox error: {e}");
        return None;
    }
    if let Err(e) = lua.globals().set("io", mlua::Value::Nil) {
        eprintln!("crux: lua sandbox error: {e}");
        return None;
    }

    // Set globals
    if let Err(e) = lua.globals().set("output", lua.create_string(output).ok()?) {
        eprintln!("crux: lua set output error: {e}");
        return None;
    }
    if let Err(e) = lua.globals().set("exit_code", exit_code) {
        eprintln!("crux: lua set exit_code error: {e}");
        return None;
    }

    let table = match lua.create_table() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("crux: lua create args table error: {e}");
            return None;
        }
    };
    for (i, arg) in args.iter().enumerate() {
        if let Err(e) = table.set(i + 1, arg.as_str()) {
            eprintln!("crux: lua set arg error: {e}");
            return None;
        }
    }
    if let Err(e) = lua.globals().set("args", table) {
        eprintln!("crux: lua set args error: {e}");
        return None;
    }

    // Execute the Lua source
    if let Err(e) = lua.load(source).exec() {
        eprintln!("crux: lua exec error: {e}");
        return None;
    }

    // Read the result global (bind before lua is dropped)
    let result = match lua.globals().get::<_, Option<String>>("result") {
        Ok(r) => r,
        Err(e) => {
            eprintln!("crux: lua get result error: {e}");
            None
        }
    };
    result
}

/// Apply a Lua filter from a file path. Reads the file, then delegates to `apply_lua`.
#[cfg(feature = "lua")]
pub fn apply_lua_file(
    file_path: &str,
    output: &str,
    exit_code: i32,
    args: &[String],
) -> Option<String> {
    match std::fs::read_to_string(file_path) {
        Ok(source) => apply_lua(&source, output, exit_code, args),
        Err(e) => {
            eprintln!("crux: lua read file error: {e}");
            None
        }
    }
}

#[cfg(test)]
#[cfg(feature = "lua")]
mod tests {
    use super::*;

    #[test]
    fn lua_sets_result() {
        let source = r#"result = output:upper()"#;
        let out = apply_lua(source, "hello world", 0, &[]);
        assert_eq!(out, Some("HELLO WORLD".to_string()));
    }

    #[test]
    fn lua_nil_passthrough() {
        let source = r#"-- do nothing, result stays nil"#;
        let out = apply_lua(source, "hello", 0, &[]);
        assert_eq!(out, None);
    }

    #[test]
    fn lua_sandbox_blocks_os_io() {
        let source = r#"result = tostring(os) .. tostring(io)"#;
        let out = apply_lua(source, "", 0, &[]);
        // os and io are nil, so tostring returns "nil"
        assert_eq!(out, Some("nilnil".to_string()));
    }
}
