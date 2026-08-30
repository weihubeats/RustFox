//! 权限一致性测试：build.rs 的命令清单必须与插件默认权限集（default.toml）一一对应。
//!
//! 背景：新增 Tauri Command 时若只改 build.rs / lib.rs 的 invoke_handler，而忘了把
//! `allow-<cmd>` 加进 `permissions/default.toml`，前端调用会报
//! `not allowed. Permissions associated with this command: fox:allow-xxx`。
//! 本测试在 `cargo test` 时即拦截该问题，无需等前端运行期才发现。

use std::fs;
use std::path::PathBuf;

/// 读取 crate 根目录下的文件（build.rs / permissions）。
fn crate_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// 提取文本中所有 `"..."` 字符串字面量。
fn quoted_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i + 1;
            if let Some(rel) = text[start..].find('"') {
                out.push(text[start..start + rel].to_string());
                i = start + rel + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 从 build.rs 的 `const COMMANDS: &[&str] = &[ ... ]` 中提取全部命令名。
fn extract_commands() -> Vec<String> {
    let source = fs::read_to_string(crate_path("build.rs")).expect("读取 build.rs");
    let start = source
        .find("const COMMANDS: &[&str] = &[")
        .expect("build.rs 中应有 COMMANDS 常量");
    let body = &source[start..];
    let end = body.find("];").expect("COMMANDS 数组应以 ]; 结束");
    quoted_strings(&body[..end])
}

/// 从 default.toml 提取 `allow-xxx` 权限名集合。
fn extract_allow_permissions() -> Vec<String> {
    let toml =
        fs::read_to_string(crate_path("permissions/default.toml")).expect("读取 default.toml");
    quoted_strings(&toml)
        .into_iter()
        .filter(|s| s.starts_with("allow-"))
        .collect()
}

#[test]
fn every_command_has_allow_permission_in_default_set() {
    let commands = extract_commands();
    assert!(!commands.is_empty(), "build.rs 命令清单不应为空");
    let allowed = extract_allow_permissions();

    let mut missing: Vec<String> = commands
        .iter()
        // 权限名为 kebab-case：get_global_variables → allow-get-global-variables
        .filter(|cmd| {
            let perm = format!("allow-{}", cmd.replace('_', "-"));
            !allowed.iter().any(|a| a == &perm)
        })
        .cloned()
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "以下命令已注册（build.rs / invoke_handler）但缺少 fox:allow-* 权限，\
         请在 crates/fox-tauri/permissions/default.toml 的 default.permissions 中补充: {:?}",
        missing,
    );
}

#[test]
fn default_set_contains_global_variable_commands() {
    let allowed = extract_allow_permissions();
    for expected in ["allow-get-global-variables", "allow-save-global-variables"] {
        assert!(
            allowed.iter().any(|a| a == expected),
            "default.toml 应包含 {expected}"
        );
    }
}
