//! 全局参数（Global Params）：每个请求自动注入的 query / header。
//!
//! 与全局变量不同：全局参数无需在请求里写 `{{name}}`，发请求时后端自动
//! 并入 query / header（请求本身已存在的同名键优先，不覆盖）。
//! 复用 settings 加密 blob 存储（`global_params` 键）。

use sqlx::SqlitePool;

use fox_core::model::GlobalParam;
use fox_core::Result;

use super::rows::decrypt_env_json;

/// settings 表存储键。
const KEY_GLOBAL_PARAMS: &str = "global_params";

/// 读取全局参数；未配置 / 解密失败返回空表。
pub async fn get_global_params(db: &SqlitePool) -> Result<Vec<GlobalParam>> {
    let Some(blob) = super::get_setting(db, KEY_GLOBAL_PARAMS).await? else {
        return Ok(Vec::new());
    };
    match decrypt_env_json(&blob) {
        Ok(value) => Ok(serde_json::from_value(value).unwrap_or_default()),
        Err(e) => {
            tracing::warn!(error = %e, "全局参数解密失败，按空表返回");
            Ok(Vec::new())
        }
    }
}

/// 保存全局参数（整体加密覆盖写）。
pub async fn save_global_params(db: &SqlitePool, params: &[GlobalParam]) -> Result<()> {
    let blob = encrypt_env_json_params(params);
    super::set_setting(db, KEY_GLOBAL_PARAMS, &blob).await
}

/// 全局参数加密（结构不同，序列化后走同一加密通道）。
fn encrypt_env_json_params(params: &[GlobalParam]) -> String {
    let json = serde_json::to_string(params).unwrap_or_else(|_| "[]".into());
    match fox_secret::ensure_master_key().and_then(|k| fox_secret::encrypt(&k, &json)) {
        Ok(cipher) => cipher,
        Err(e) => {
            tracing::warn!(error = %e, "全局参数加密失败，已降级为明文存储");
            json
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memory_pool;
    use fox_core::model::GlobalParamLocation;

    #[tokio::test]
    async fn global_params_roundtrip_encrypted() {
        let db = memory_pool().await.unwrap();
        assert!(get_global_params(&db).await.unwrap().is_empty());

        let params = vec![
            GlobalParam {
                key: "X-Request-Id".into(),
                value: "trace-123".into(),
                enabled: true,
                location: GlobalParamLocation::Header,
            },
            GlobalParam {
                key: "debug".into(),
                value: "1".into(),
                enabled: true,
                location: GlobalParamLocation::Query,
            },
            GlobalParam {
                key: "off".into(),
                value: "x".into(),
                enabled: false,
                location: GlobalParamLocation::Header,
            },
        ];
        save_global_params(&db, &params).await.unwrap();

        let raw: (String,) =
            sqlx::query_as("SELECT value_json FROM settings WHERE key = 'global_params'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(!raw.0.contains("trace-123"), "全局参数应加密存储");

        let loaded = get_global_params(&db).await.unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].key, "X-Request-Id");
        assert_eq!(loaded[0].location, GlobalParamLocation::Header);
        assert_eq!(loaded[1].location, GlobalParamLocation::Query);
        assert!(!loaded[2].enabled);
    }
}
