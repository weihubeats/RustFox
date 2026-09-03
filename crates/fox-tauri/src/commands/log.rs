//! 日志查看 Command：日志文件列表 / 尾部读取 / 目录路径。
//!
//! 日志由 `init_tracing` 按天滚动（`{log_dir}/rustfox.log.YYYY-MM-DD`）。
//! 原来只能靠顶栏「反馈」生成一次性诊断摘要；本模块给设置页"日志" Tab
//! 提供查看面，排障不再需要用户翻文件系统。

use serde::Serialize;

use crate::error::{CommandError, CommandResult};

/// 日志文件元信息（按修改时间倒序）。
#[derive(Debug, Clone, Serialize)]
pub struct LogFile {
    pub name: String,
    pub size_bytes: u64,
    pub modified_at: String,
}

/// 列出日志文件（最新在前；目录缺失返回空）。
#[tauri::command(rename_all = "camelCase")]
pub async fn log_files() -> CommandResult<Vec<LogFile>> {
    let dir = fox_storage::db::log_dir();
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok(out);
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("rustfox.log") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() || path.extension().is_some_and(|e| e == "gz") {
            continue;
        }
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339().into())
            .unwrap_or_default();
        out.push(LogFile {
            name,
            size_bytes: meta.len(),
            modified_at,
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// 读取日志尾部（默认 300 行，上限 2000 行；只读末尾 512KB，大文件不爆内存）。
#[tauri::command(rename_all = "camelCase")]
pub async fn log_tail(file: Option<String>, lines: Option<u64>) -> CommandResult<String> {
    let dir = fox_storage::db::log_dir();
    let name = file.unwrap_or_else(|| "rustfox.log".to_string());
    // 文件名限定：防目录穿越。
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(CommandError::validation("非法的文件名"));
    }
    let path = dir.join(&name);
    let content = std::fs::read(&path)
        .map_err(|e| CommandError::with_code("IO", format!("读取日志失败（{name}）：{e}")))?;
    const MAX_TAIL_BYTES: usize = 512 * 1024;
    const MAX_LINES: u64 = 2000;
    let want = lines.unwrap_or(300).clamp(1, MAX_LINES) as usize;
    // 取末尾字节（字符边界安全），再按行截取。
    let start = content.len().saturating_sub(MAX_TAIL_BYTES);
    let mut start = start;
    while start < content.len() && !is_utf8_boundary(&content, start) {
        start += 1;
    }
    let text = String::from_utf8_lossy(&content[start..]);
    let all: Vec<&str> = text.lines().collect();
    let from = all.len().saturating_sub(want);
    Ok(all[from..].join("\n"))
}

fn is_utf8_boundary(bytes: &[u8], index: usize) -> bool {
    if index >= bytes.len() {
        return true;
    }
    (bytes[index] as i8) >= -0x40
}

/// 日志目录绝对路径（供「打开目录」）。
#[tauri::command(rename_all = "camelCase")]
pub async fn log_dir_path() -> CommandResult<String> {
    Ok(fox_storage::db::log_dir().to_string_lossy().to_string())
}
