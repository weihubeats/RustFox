//! 数据库连接、路径与迁移。

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use fox_core::{AppError, Result};

/// {SystemDataDir}/RustFox（开发构建用 RustFox-dev，与正式版数据隔离：
/// 避免 tauri dev 跑过更新的迁移后，旧正式版打开同一数据库因迁移版本
/// 校验失败而启动即退出）
pub fn data_dir() -> PathBuf {
    let sub = if cfg!(debug_assertions) {
        "RustFox-dev"
    } else {
        "RustFox"
    };
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(sub)
}

/// 日志目录 {SystemDataDir}/RustFox/logs
pub fn log_dir() -> PathBuf {
    data_dir().join("logs")
}

/// 数据库文件路径。
pub fn database_path() -> PathBuf {
    data_dir().join("rustfox.db")
}

/// 开发构建（debug）启动前删除数据库文件（含 WAL / SHM）：迁移重建后由
/// `seed::seed_dev_data` 写入一致的测试数据集，`npm run tauri dev`
/// 每次重启都从干净数据开始。release 构建为空操作，不影响正式数据。
pub fn reset_dev_database() {
    if !cfg!(debug_assertions) {
        return;
    }
    let path = database_path();
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.clone().into_os_string();
        p.push(suffix);
        let _ = std::fs::remove_file(&p);
    }
}

/// 快照保留目录（数据目录下 `snapshots/`）。
pub fn snapshot_dir() -> PathBuf {
    data_dir().join("snapshots")
}

/// 快照保留份数（超出按最旧删除）。
pub const SNAPSHOT_RETENTION: usize = 5;

/// 建立连接并执行迁移。
///
/// - 迁移前：已存在库文件先 WAL checkpoint + 全量快照（`snapshots/` 保留 5 份），
///   升级失败可回滚，之前是"裸跑迁移，坏了只能靠 backups/ JSON 重建"；
/// - 迁移后：`PRAGMA integrity_check` 自检，非 ok 直接报错并指引恢复路径。
pub async fn init_db(path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let fresh = !path.exists();
    let pool = connect(path).await?;
    if !fresh {
        snapshot_database(&pool, path).await?;
    }
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(sqlx::Error::Protocol(e.to_string())))?;
    integrity_check(&pool, path).await?;
    Ok(pool)
}

/// 迁移前快照：checkpoint 落盘 + 复制库文件 + 修剪旧快照（失败仅告警，不阻断启动）。
async fn snapshot_database(pool: &SqlitePool, path: &Path) -> Result<()> {
    // data_dir() 在测试/正式环境下不同：快照落在库文件同级 `snapshots/`，
    // 避免测试污染正式目录、也避免多环境互相覆盖。
    let dir = path
        .parent()
        .map(|p| p.join("snapshots"))
        .unwrap_or_else(snapshot_dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("创建快照目录失败（跳过快照）：{e}");
        return Ok(());
    }
    // 同步 checkpoint，保证快照文件自包含（WAL 内容并回主库）。
    let _: Option<(i32, i32, i32)> = sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("rustfox-{stamp}.db"));
    if let Err(e) = std::fs::copy(path, &dest) {
        tracing::warn!("迁移前快照失败（继续启动）：{e}");
        return Ok(());
    }
    // 修剪：仅保留最近 SNAPSHOT_RETENTION 份。
    let mut snaps: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("rustfox-") && n.ends_with(".db"))
                })
                .collect()
        })
        .unwrap_or_default();
    snaps.sort();
    while snaps.len() > SNAPSHOT_RETENTION {
        let Some(old) = snaps.first().cloned() else {
            break;
        };
        if let Err(e) = std::fs::remove_file(&old) {
            tracing::warn!("清理旧快照失败：{e}");
            break;
        }
        snaps.remove(0);
    }
    tracing::info!("迁移前快照已保存：{}", dest.display());
    Ok(())
}

/// 迁移后自检：`PRAGMA integrity_check` 非 ok 即报错并指引恢复。
async fn integrity_check(pool: &SqlitePool, path: &Path) -> Result<()> {
    let row: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(pool)
        .await?;
    if row.0.eq_ignore_ascii_case("ok") {
        return Ok(());
    }
    Err(AppError::Database(sqlx::Error::Protocol(format!(
        "数据库完整性检查失败（{}）：请用设置页备份 JSON 恢复，或从 snapshots/ 复制快照回 {} 后重启",
        row.0.lines().next().unwrap_or(&row.0),
        path.display()
    ))))
}

/// 只建连接，不跑迁移（测试用）。
pub async fn connect(path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        // WAL 下的推荐档位：崩溃不丢事务，且写入时不再每笔 fsync（性能差异可达数倍）
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// 内存数据库（测试用）。
pub async fn memory_pool() -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(":memory:")
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(sqlx::Error::Protocol(e.to_string())))?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 二次启动：已存在库 → 快照 1 份 + integrity_check 通过。
    #[tokio::test]
    async fn init_db_snapshots_and_self_checks() {
        let dir = std::env::temp_dir().join(format!("rustfox-db-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("rustfox.db");
        let pool = init_db(&path).await.expect("首次建库");
        pool.close().await;
        let pool = init_db(&path).await.expect("二次启动应快照+自检通过");
        pool.close().await;
        let snaps: Vec<_> = std::fs::read_dir(dir.join("snapshots"))
            .expect("快照目录")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(snaps.len(), 1, "应产生 1 份快照");
        assert!(snaps[0]
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with("rustfox-") && n.ends_with(".db")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
