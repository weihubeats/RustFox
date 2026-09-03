-- 性能补索引（历史查询拆分 + 测试历史 + Mock/目录常用过滤）。
--
-- 1. request_histories 按接口过滤：`WHERE project_id = ? AND endpoint_id = ?`
--    ORDER BY created_at DESC（0003 的 (project_id, created_at) 覆盖不了 endpoint 条件）。
CREATE INDEX IF NOT EXISTS idx_histories_project_endpoint_created
    ON request_histories (project_id, endpoint_id, created_at DESC);

-- 2. test_runs 按项目查最近运行：`WHERE project_id = ? ORDER BY started_at DESC`。
CREATE INDEX IF NOT EXISTS idx_test_runs_project_started
    ON test_runs (project_id, started_at DESC);

-- 3. mock_rules 按接口关联查询（Mock 管理面板）。
CREATE INDEX IF NOT EXISTS idx_mock_rules_endpoint
    ON mock_rules (endpoint_id);

-- 4. folders 按父目录查子树（目录树展开 / 级联删除 CTE）。
CREATE INDEX IF NOT EXISTS idx_folders_parent
    ON folders (parent_id);
