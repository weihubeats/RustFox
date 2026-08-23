-- 响应示例按接口过滤（示例面板 / 备份导出逐接口 list_response_examples），
-- 与 0004 的 request_examples 索引对称，避免全表扫描。
CREATE INDEX IF NOT EXISTS idx_response_examples_endpoint
    ON response_examples(endpoint_id);
