-- 环境提升为全局维度：去掉 project_id 归属，跨项目共享。
-- 模块（modules_json）改为与项目联动（ModuleUrlConfig.project_id）。
--
-- SQLite 无 DROP COLUMN 旧版本兼容路径，用「建新表 + 拷贝 + 重命名」重建。
-- test_runs.environment_id 外键引用 environments(id) 按表名解析，重命名后不受影响。
CREATE TABLE environments_v2 (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    variables_json TEXT NOT NULL DEFAULT '[]',
    modules_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO environments_v2 (id, name, variables_json, modules_json, created_at, updated_at)
SELECT id, name, variables_json, modules_json, created_at, updated_at FROM environments;

DROP TABLE environments;
ALTER TABLE environments_v2 RENAME TO environments;

DROP INDEX IF EXISTS idx_environments_project;