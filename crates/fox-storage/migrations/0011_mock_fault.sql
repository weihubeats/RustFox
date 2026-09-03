-- Mock 故障注入：按命中比例返回故障状态码。
-- 旧库缺列时补齐（默认关闭：fault_rate_pct = 0，fault_status = 500）。
ALTER TABLE mock_rules ADD COLUMN fault_rate_pct INTEGER NOT NULL DEFAULT 0;
ALTER TABLE mock_rules ADD COLUMN fault_status INTEGER NOT NULL DEFAULT 500;
