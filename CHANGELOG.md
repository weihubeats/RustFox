# CHANGELOG

版本变化速览（完整提交记录见 `git log`，发版流程见 `docs/RELEASE.md`）。

## Unreleased（性能与功能优化合集）

性能：

- 导出/备份/Mock 示例查询去 N+1（单次 IN 查询按接口分组）
- Mock 路由索引化（预编译 + (method, 段数) 分组 + 单次 query 解析）
- 压测改固定 worker 池（句柄数与 total 解耦）+ 超时透传 + 可取消 + 64KB body 上限
- 文件上传流式化（multipart/binary 边读边发，大文件不进内存）
- 备份恢复事务化（BEGIN IMMEDIATE，失败回滚）
- 请求热路径三查询同波次并发 + 历史裁剪节流 + seq 脏检查落盘 + 变量表单次合并
- 新增性能索引（test_runs、histories endpoint 过滤、mock_rules endpoint、folders parent）
- 响应渲染懒计算（树接管时跳过 stringify/切分）+ JsonTree O(1) 展开 + 查找 DOM 级激活
- 编辑器高亮防抖 + CodeMirror 回写防抖 + Body 纯文本镜像防抖
- 目录树共享子索引 + Tab 栏单遍求值 + 切 Tab 缓存命中跳过 IPC
- 压测图表 150ms 节流 + LTTB 抽样 + 首屏分包（vue/codemirror/chart 独立 chunk）

功能：

- 实时调试视图（WebSocket 收发/Ping/自动重连 + SSE 订阅/帧解析/续传 + 独立窗口弹出）
- 启动迁移前快照（保留 5 份）+ integrity_check 自检
- 环境导入导出（RustFox 原生 / Postman Environment）
- Cookie 管理（自管 Jar：侧栏查看/按域清理 + 单请求禁用）
- 历史搜索（关键字 + 状态筛选）+ Mock 热重载 + 故障注入（比例/状态码）
- 断言扩展（matches/regex、empty、graphql_errors、length）
- 删除撤销（Toast 8 秒）+ 目录树多选批量删除/移动
- 快捷键集中注册表 + 帮助面板（Ctrl+/）
- 设置页日志查看 + OpenAPI 3.1 导入（归一化为 3.0 子集）
- 备份扩域（全局设置快照 + 全局变量/参数保守合并）+ cURL 忽略参数预览 + Rust/PHP 代码导入

## v0.0.12

- 主题三档：跟随系统 / 深色 / 浅色（含设置弹窗浅色适配，持久化 + 防闪烁）
- 请求超时可配置 + 自增序列管理 + 设置页重构
- 更新进度条累计修正；备份 / 文档导出改目录选择框
- dev 种子数据默认激活公网接口，开箱即可发送请求

## v0.0.11

- 环境全局化：多模块 Base URL + 全局变量/参数；默认模块随当前项目
- 环境管理器语义对齐（移除兜底单选列，修复保存双 toast / 行尾空条）

## v0.0.10

- 安装包内置 `rustfox-mcp` 侧载二进制（macOS/Windows/Linux）；MCP 文档更新为安装包内绝对路径
- 多项目标签页：顶栏快照切换，草稿/标签跨项目保留
- 响应 JSON 树默认全展开；请求区未发送时占满高度

## v0.0.9

- Agent 集成：本机控制面 HTTP API（4110 起探测）+ `rustfox-mcp` CLI + 前端事件刷新
- 新增 `docs/AGENT.md` 完整指南；官网落地页 + GitHub Pages 自动部署
- Mock 管理弹层渲染修复 + 回归测试

## v0.0.8

- 设计态 Schema 标注 + 多格式文档导出；全链路性能与健壮性优化
- 大响应/大文档渲染内存防护（GraphQL 上限、JsonTree 行数上限、编辑器大内容降级）
- 品牌图标资源更新

## v0.0.7

- 测试用例 Drawer 重构（Method 联动 / CodeMirror 6 / 拖拽分割）
- 项目卡片拖拽排序持久化；激活项目/环境持久化到 settings 表（重启恢复）
- URL 栏显示解析后域名；编辑器体验升级（历史侧栏化、多语言代码导入、binary 请求体）

## v0.0.6 及更早

- Cookie 自动回放 + 全局代理设置；请求历史按接口过滤
- 大响应渲染保护；开发/正式版数据目录隔离
- v0.0.3 起应用内自动更新（关于 → Check for Updates）
