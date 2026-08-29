-- 环境多模块：新增 modules_json 列。
-- 兼容：旧环境无该列，读端按空模块列表处理（variables_json 旧 map 格式在 rows.rs 内做兼容转换）。
ALTER TABLE environments ADD COLUMN modules_json TEXT NOT NULL DEFAULT '[]';