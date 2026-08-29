//! IPC 权限契约测试：注册的命令必须已登记权限清单。
//!
//! 背景：list_project_stats 注册进了 `generate_handler!`，但漏登
//! build.rs COMMANDS 与 permissions/default.toml，导致 Tauri 运行时
//! 权限层拒绝前端调用（invoke 报 not allowed），仪表板统计静默归零。
//! 本测试直接扫描三份源文件，任何「注册了命令却没配权限」的改动立即红灯。

use std::collections::BTreeSet;

const LIB_RS: &str = include_str!("../src/lib.rs");
const BUILD_RS: &str = include_str!("../build.rs");
const DEFAULT_TOML: &str = include_str!("../permissions/default.toml");

/// 从 lib.rs 的 `invoke_handler(tauri::generate_handler![...])` 块提取命令名。
fn handler_commands() -> BTreeSet<String> {
    let start = LIB_RS
        .find("generate_handler![")
        .expect("lib.rs 应包含 generate_handler! 块");
    let end = LIB_RS[start..]
        .find(']')
        .expect("generate_handler! 块未闭合");
    let block = &LIB_RS[start..start + end];
    block
        .split("commands::")
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .collect()
}

/// 从 build.rs 的 `const COMMANDS` 清单提取命令名。
fn manifest_commands() -> BTreeSet<String> {
    let start = BUILD_RS
        .find("const COMMANDS: &[&str] = &[")
        .expect("build.rs 应包含 COMMANDS 清单");
    let end = BUILD_RS[start..].find("];").expect("COMMANDS 清单未闭合");
    BUILD_RS[start..start + end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[test]
fn every_registered_command_has_permission() {
    let handlers = handler_commands();
    assert!(
        handlers.len() >= 60,
        "generate_handler 解析异常，命令数过少：{}",
        handlers.len()
    );

    let manifest = manifest_commands();

    // 1. generate_handler 与 build.rs COMMANDS 逐一对应（双向）
    let missing_in_manifest: Vec<_> = handlers.difference(&manifest).collect();
    assert!(
        missing_in_manifest.is_empty(),
        "以下命令注册了 handler 但漏登 build.rs COMMANDS（权限不会生成，运行时被拒）：{missing_in_manifest:?}"
    );
    let ghost_in_manifest: Vec<_> = manifest.difference(&handlers).collect();
    assert!(
        ghost_in_manifest.is_empty(),
        "build.rs COMMANDS 存在未注册的命令（清单与 handler 不同步）：{ghost_in_manifest:?}"
    );

    // 2. 每个命令在 permissions/default.toml 有对应的 allow-<kebab-case>
    let mut missing_in_toml = Vec::new();
    for cmd in &handlers {
        let allow = format!("allow-{}", cmd.replace('_', "-"));
        if !DEFAULT_TOML.contains(&allow) {
            missing_in_toml.push(allow);
        }
    }
    assert!(
        missing_in_toml.is_empty(),
        "以下命令未加入 permissions/default.toml 默认权限集（前端 invoke 会被拒绝）：{missing_in_toml:?}"
    );
}
