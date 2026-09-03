//! ResponseExample（响应示例）。

use std::collections::HashMap;

use sqlx::{QueryBuilder, SqlitePool};
use uuid::Uuid;

use fox_core::model::ResponseExample;
use fox_core::Result;

use super::rows::ResponseExampleRow;

pub async fn create_response_example<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    endpoint_id: Uuid,
    example: &ResponseExample,
) -> Result<ResponseExample> {
    let row = ResponseExampleRow::from_model(example);
    sqlx::query(
        "INSERT INTO response_examples
             (id, endpoint_id, name, status, headers_json, body, content_type, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.endpoint_id)
    .bind(&row.name)
    .bind(row.status)
    .bind(row.headers_json.clone())
    .bind(&row.body)
    .bind(&row.content_type)
    .bind(row.created_at.clone())
    .bind(row.updated_at.clone())
    .execute(executor)
    .await?;
    let _ = endpoint_id;
    Ok(example.clone())
}

pub async fn list_response_examples(
    db: &SqlitePool,
    endpoint_id: Uuid,
) -> Result<Vec<ResponseExample>> {
    let rows: Vec<ResponseExampleRow> = sqlx::query_as(
        "SELECT id, endpoint_id, name, status, headers_json, body, content_type, created_at, updated_at
         FROM response_examples WHERE endpoint_id = ? ORDER BY created_at",
    )
    .bind(endpoint_id.to_string())
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(ResponseExampleRow::into_model)
        .collect()
}

/// 批量列出多个接口的响应示例（一次查询按 endpoint 分组；导出/备份/Mock 去 N+1 用）。
///
/// SQLite 变量数上限按 500 分片，避免超长 IN 列表。
pub async fn list_response_examples_by_endpoints(
    db: &SqlitePool,
    endpoint_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<ResponseExample>>> {
    let mut map: HashMap<Uuid, Vec<ResponseExample>> = HashMap::with_capacity(endpoint_ids.len());
    for chunk in endpoint_ids.chunks(500) {
        if chunk.is_empty() {
            continue;
        }
        let mut qb = QueryBuilder::new(
            "SELECT id, endpoint_id, name, status, headers_json, body, content_type, created_at, updated_at
             FROM response_examples WHERE endpoint_id IN (",
        );
        let mut separated = qb.separated(", ");
        for id in chunk {
            separated.push_bind(id.to_string());
        }
        separated.push_unseparated(") ORDER BY endpoint_id, created_at");
        let rows: Vec<ResponseExampleRow> = qb.build_query_as().fetch_all(db).await?;
        for row in rows {
            let model = row.into_model()?;
            map.entry(model.endpoint_id).or_default().push(model);
        }
    }
    Ok(map)
}

/// 删除单条响应示例（M10 示例管理）。
pub async fn delete_response_example(db: &SqlitePool, example_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM response_examples WHERE id = ?")
        .bind(example_id.to_string())
        .execute(db)
        .await?;
    Ok(())
}

/// 删除某接口的全部响应示例（导入覆盖时使用）。
pub async fn delete_response_examples(db: &SqlitePool, endpoint_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM response_examples WHERE endpoint_id = ?")
        .bind(endpoint_id.to_string())
        .execute(db)
        .await?;
    Ok(())
}

/// 带 id：原样写入响应示例。
pub async fn save_response_example<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    example: &ResponseExample,
) -> Result<()> {
    create_response_example(executor, example.endpoint_id, example)
        .await
        .map(|_| ())
}
