//! Request History。

use sqlx::SqlitePool;
use uuid::Uuid;

use fox_core::model::RequestHistory;
use fox_core::Result;

use super::rows::HistoryRow;

/// 每个项目保留的历史条数上限（超出按最旧淘汰）。
///
/// 历史每条含请求/响应摘要 JSON，无限增长会持续膨胀数据库并拖慢
/// 列表查询与启动迁移，故写入时顺带按项目裁剪。
pub const HISTORY_RETENTION_PER_PROJECT: i64 = 500;

/// 裁剪节流计数：每 N 次写入做一次保留策略裁剪，而非逐次双写。
/// 逐次裁剪意味着请求热路径每次 2 次写（INSERT + 全表 DELETE…NOT IN）。
const TRIM_EVERY_INSERTS: u64 = 20;
static INSERT_TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub async fn save_request_history(db: &SqlitePool, model: &RequestHistory) -> Result<()> {
    let row = HistoryRow::from_model(model);
    sqlx::query(
        "INSERT INTO request_histories
         (id, project_id, endpoint_id, method, url, status, duration_ms, request_summary_json, response_summary_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.project_id)
    .bind(&row.endpoint_id)
    .bind(&row.method)
    .bind(&row.url)
    .bind(row.status)
    .bind(row.duration_ms)
    .bind(&row.request_summary_json)
    .bind(&row.response_summary_json)
    .bind(row.created_at.clone())
    .execute(db)
    .await?;
    // 保留策略节流：超额最多延迟 N 条被清理，换热路径少一次写。
    let tick = INSERT_TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if tick % TRIM_EVERY_INSERTS == 0 {
        trim_request_history(db, &row.project_id).await?;
    }
    Ok(())
}

/// 保留策略：淘汰指定项目最旧的超额历史（`HISTORY_RETENTION_PER_PROJECT` 之外）。
pub async fn trim_request_history(db: &SqlitePool, project_id: &str) -> Result<()> {
    sqlx::query(
        "DELETE FROM request_histories
         WHERE project_id = ? AND id NOT IN (
             SELECT id FROM request_histories WHERE project_id = ?
             ORDER BY created_at DESC LIMIT ?
         )",
    )
    .bind(project_id)
    .bind(project_id)
    .bind(HISTORY_RETENTION_PER_PROJECT)
    .execute(db)
    .await?;
    Ok(())
}

/// 查询项目请求历史（时间倒序）；`endpoint_id` 为 Some 时仅返回该接口的记录。
///
/// 按有无 endpoint 条件拆两条 SQL：`(? IS NULL OR endpoint_id = ?)` 的 OR
/// 会让 endpoint 过滤走不上索引，全项目历史一多按接口过滤就全表扫描。
pub async fn list_request_histories(
    db: &SqlitePool,
    project_id: Uuid,
    endpoint_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<RequestHistory>> {
    const BASE: &str = "SELECT id, project_id, endpoint_id, method, url, status, duration_ms,
                request_summary_json, response_summary_json, created_at
         FROM request_histories";
    let rows: Vec<HistoryRow> = match endpoint_id {
        Some(ep_id) => {
            sqlx::query_as(&format!(
                "{BASE} WHERE project_id = ? AND endpoint_id = ? ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(project_id.to_string())
            .bind(ep_id.to_string())
            .bind(limit)
            .fetch_all(db)
            .await?
        }
        None => {
            sqlx::query_as(&format!(
                "{BASE} WHERE project_id = ? ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(project_id.to_string())
            .bind(limit)
            .fetch_all(db)
            .await?
        }
    };
    rows.into_iter().map(HistoryRow::into_model).collect()
}

/// 清空项目请求历史；`endpoint_id` 为 Some 时仅清该接口的记录。返回删除条数。
pub async fn clear_request_histories(
    db: &SqlitePool,
    project_id: Uuid,
    endpoint_id: Option<Uuid>,
) -> Result<u64> {
    let result = sqlx::query(
        "DELETE FROM request_histories WHERE project_id = ? AND (? IS NULL OR endpoint_id = ?)",
    )
    .bind(project_id.to_string())
    .bind(endpoint_id.map(|id| id.to_string()))
    .bind(endpoint_id.map(|id| id.to_string()))
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}
