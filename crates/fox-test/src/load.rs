//! M14：接口压测（并发基准测试）。
//!
//! 调度模型：固定 `concurrency` 个 worker 任务 + 原子计数器领任务，
//! 结果经 mpsc 回传聚合。任务句柄数恒为并发数（原来 `spawn(total)`，
//! total=10 万时 10 万句柄常驻），内存占用与总请求数解耦。

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Instant;

use fox_core::model::{HttpMethod, RequestSpec};
use tokio::sync::mpsc;

use fox_http::client::send_request_capped;

/// 压测配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct LoadConfig {
    /// 并发数（同时进行的请求数）。
    pub concurrency: usize,
    /// 总请求数。
    pub total: usize,
}

/// 压测结果。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LoadResult {
    pub total: usize,
    pub ok: usize,
    pub failed: usize,
    pub total_ms: u64,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p99_ms: f64,
    pub rps: f64,
    /// 最多保留 5 条请求错误。
    pub errors: Vec<String>,
    /// 是否被用户中途取消（取消时 total < 配置的 total）。
    #[serde(default)]
    pub cancelled: bool,
}

/// 压测运行选项（`run_load` 保持旧默认行为；精细控制用 `run_load_with`）。
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// 单请求超时毫秒数；None 时走 fox-http 默认。
    pub timeout_ms: Option<u64>,
    /// 取消令牌；触发后 worker 停止领新任务、在途请求中止。
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    /// 单响应体读取上限；默认 64KB（压测只关心状态码/耗时，
    /// 避免 500 并发 × 大响应体的内存爆炸；耗时口径为"到截断为止"）。
    pub body_cap_bytes: Option<usize>,
}

impl LoadOptions {
    fn body_cap(&self) -> usize {
        self.body_cap_bytes.unwrap_or(64 * 1024)
    }
}

fn percentile(sorted: &[f64], q: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (sorted.len() * q / 100).min(sorted.len() - 1);
    sorted[idx]
}

/// 压测进度快照（回调参数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct LoadProgress {
    pub done: usize,
    pub total: usize,
    pub ok: usize,
    pub failed: usize,
}

/// 并发压测：`total` 次请求，最多 `concurrency` 个同时进行。
/// `progress` 可选：每完成一次请求回调一次快照（用于进度推送）。
///
/// 超时走 fox-http 默认，body 上限 64KB，不可取消；需要精细控制用
/// [`run_load_with`]。
pub async fn run_load(
    method: HttpMethod,
    url: &str,
    spec: &RequestSpec,
    cfg: &LoadConfig,
    progress: Option<&(dyn Fn(LoadProgress) + Send + Sync)>,
) -> LoadResult {
    run_load_with(method, url, spec, cfg, &LoadOptions::default(), progress).await
}

/// 单次请求采样（worker → 聚合器）。
struct Sample {
    ok: bool,
    duration_ms: f64,
    err: Option<String>,
}

/// 精细版并发压测：固定 worker 池 + 共享请求规格 + 超时透传 + 可取消。
///
/// - `spec`/`url` 经 `Arc` 在 worker 间共享，不再逐请求深拷贝；
/// - worker 数恒为 `concurrency`，句柄与内存占用和 `total` 解耦；
/// - 取消后已完成样本保留，`cancelled=true`，`total=ok+failed`。
pub async fn run_load_with(
    method: HttpMethod,
    url: &str,
    spec: &RequestSpec,
    cfg: &LoadConfig,
    options: &LoadOptions,
    progress: Option<&(dyn Fn(LoadProgress) + Send + Sync)>,
) -> LoadResult {
    let concurrency = cfg.concurrency.clamp(1, 500).max(1);
    let total = cfg.total.clamp(1, 100_000).max(1);
    let start = Instant::now();
    let spec = Arc::new(spec.clone());
    let url: Arc<str> = Arc::from(url);
    let next = Arc::new(AtomicUsize::new(0));
    let body_cap = options.body_cap();
    let timeout_ms = options.timeout_ms;
    let cancel = options.cancel.clone();

    // 缓冲 = worker 数 × 2：背压天然存在，无需额外信号量。
    let (tx, mut rx) = mpsc::channel::<Sample>(concurrency * 2);
    for _ in 0..concurrency {
        let tx = tx.clone();
        let next = next.clone();
        let spec = spec.clone();
        let url = url.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
                    break;
                }
                let idx = next.fetch_add(1, Ordering::Relaxed);
                if idx >= total {
                    break;
                }
                let t = Instant::now();
                let r =
                    send_request_capped(method, &url, &spec, timeout_ms, cancel.as_ref(), body_cap)
                        .await;
                let d = t.elapsed().as_secs_f64() * 1000.0;
                let sample = match r {
                    Ok(_) => Sample {
                        ok: true,
                        duration_ms: d,
                        err: None,
                    },
                    Err(e) => Sample {
                        ok: false,
                        duration_ms: d,
                        err: Some(e.user_message()),
                    },
                };
                if tx.send(sample).await.is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    let mut samples: Vec<f64> = Vec::new();
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut done = 0usize;
    while let Some(s) = rx.recv().await {
        done += 1;
        samples.push(s.duration_ms);
        if s.ok {
            ok += 1;
        } else {
            failed += 1;
        }
        if let Some(e) = s.err {
            if errors.len() < 5 {
                errors.push(e);
            }
        }
        if let Some(cb) = progress {
            cb(LoadProgress {
                done,
                total,
                ok,
                failed,
            });
        }
    }

    let cancelled = cancel.as_ref().is_some_and(|c| c.is_cancelled());
    let total_ms = start.elapsed().as_millis() as u64;
    let mut sorted = samples.clone();
    sorted.sort_unstable_by(|a, b| a.total_cmp(b));
    let done = total_ms.max(1);
    LoadResult {
        total: ok + failed,
        ok,
        failed,
        total_ms,
        avg_ms: if samples.is_empty() {
            0.0
        } else {
            samples.iter().sum::<f64>() / samples.len() as f64
        },
        p50_ms: percentile(&sorted, 50),
        p90_ms: percentile(&sorted, 90),
        p99_ms: percentile(&sorted, 99),
        rps: (ok + failed) as f64 * 1000.0 / done as f64,
        errors,
        cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fox_core::model::KeyValue;
    use tokio::net::TcpListener;

    #[test]
    fn percentile_basic() {
        assert_eq!(percentile(&[], 50), 0.0);
        assert_eq!(percentile(&[10.0, 20.0, 30.0, 40.0], 50), 30.0);
        assert_eq!(percentile(&[10.0, 20.0, 30.0, 40.0], 90), 40.0);
        assert_eq!(percentile(&[5.0], 99), 5.0);
    }

    #[tokio::test]
    async fn run_load_basic() {
        let app = axum::Router::new().route("/ping", axum::routing::get(|| async { "pong" }));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let spec = RequestSpec {
            params: vec![KeyValue::new("x", "1")],
            ..Default::default()
        };
        let cfg = LoadConfig {
            concurrency: 4,
            total: 20,
        };
        let result = run_load(
            HttpMethod::GET,
            &format!("http://{addr}/ping"),
            &spec,
            &cfg,
            None,
        )
        .await;
        assert_eq!(result.total, 20, "总请求数应等于配置");
        assert_eq!(result.failed, 0, "本地服务不应失败");
        assert_eq!(result.ok, 20);
        assert!(result.avg_ms >= 0.0);
        assert!(result.rps > 0.0);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn run_load_with_cancel_stops_early() {
        use std::time::Duration;
        // 慢服务：取消后不应跑满 total。
        let app = axum::Router::new().route(
            "/slow",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                "slow"
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let cancel = tokio_util::sync::CancellationToken::new();
        let canceller = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            canceller.cancel();
        });
        let options = LoadOptions {
            timeout_ms: Some(5_000),
            cancel: Some(cancel),
            body_cap_bytes: Some(1024),
        };
        let cfg = LoadConfig {
            concurrency: 2,
            total: 50,
        };
        let result = run_load_with(
            HttpMethod::GET,
            &format!("http://{addr}/slow"),
            &RequestSpec::default(),
            &cfg,
            &options,
            None,
        )
        .await;
        assert!(result.cancelled, "取消后应标记 cancelled");
        assert!(result.total < 50, "取消后不应跑满，实际 {}", result.total);
    }

    #[tokio::test]
    async fn run_load_handles_failures() {
        let addr = "127.0.0.1:9"; // discard 端口，连接必然失败。
        let spec = RequestSpec::default();
        let cfg = LoadConfig {
            concurrency: 2,
            total: 6,
        };
        let result = run_load(
            HttpMethod::GET,
            &format!("http://{addr}/nope"),
            &spec,
            &cfg,
            None,
        )
        .await;
        assert_eq!(result.total, 6);
        assert_eq!(result.ok, 0);
        assert_eq!(result.failed, 6);
        assert!(!result.errors.is_empty());
        assert!(result.errors.len() <= 5);
    }
}
