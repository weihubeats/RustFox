# RustFox 里程碑

里程碑总览，详细进度记录见 [docs/PROGRESS.md](PROGRESS.md)，完整规范见 [docs/SPEC.md](SPEC.md)。

| 里程碑 | 内容 | 状态 |
| --- | --- | --- |
| M0 | 仓库初始化 | ✅ 完成 |
| M1 | 核心模型与数据库 | ✅ 完成 |
| M2 | 桌面应用骨架 | ✅ 完成 |
| M2.5 | 设计系统重构（rf- 类名、CSS 变量、SVG 图标） | ✅ 完成 |
| M2.6 | 功能联通性验收与诊断（冒烟测试、反馈报告） | ✅ 完成 |
| M3 | 目录树与接口管理 | ✅ 完成 |
| M4 | 接口编辑器 | ✅ 完成 |
| M5 | HTTP 调试 | ✅ 完成 |
| M6 | 环境与变量 | ✅ 完成 |
| M7 | OpenAPI 导入导出 | ✅ 完成 |
| M8 | Mock Server | ✅ 完成 |
| M9 | 自动化测试 | ✅ 完成 |
| M10 | 文档与备份 | ✅ 完成 |
| M11 | 测试历史 / 变量加密 / 部署文档 | ✅ 完成 |
| M12 | 导入兼容（Swagger 2.0 / Postman v2.1） | ✅ 完成 |
| M13 | 客户端代码生成（curl / Python / JS / Go） | ✅ 完成 |
| M14 | 接口压测（并发基准：QPS / 分位耗时） | ✅ 完成 |
| M15 | 多标签编辑（独立草稿 / 未保存标记 / 新建标签） | ✅ 完成 |
| M16 | UI 视觉翻新 | ✅ 完成 |
| M17 | Tauri 2 迁移（Dioxus → Vue 3 + Tauri 插件 `fox`，40+ IPC 命令） | ✅ 完成 |
| M18 | Agent 集成（本机控制面 4110 + `rustfox-mcp`：save_curl / list_projects / list_endpoints / agent_info） | ✅ 完成 |
| M19 | 官网落地页（中英双语静态页 + GitHub Pages 自动部署） | ✅ 完成 |
| M20 | 环境全局化（多模块 Base URL + 全局变量/参数，默认模块随项目走） | ✅ 完成 |
| M21 | 工作区体验（多项目标签快照 / GraphQL 调试视图 / 测试用例 Drawer / 设计态 Schema / Cookie 回放 + 全局代理） | ✅ 完成 |
| M22 | 偏好与外观（主题 跟随系统/深色/浅色 / 请求超时可配置 / 自增序列管理 / 备份导出目录选择） | ✅ 完成 |

> M17 起为 Tauri 时代里程碑，Dioxus 相关行（M0–M2 的桌面描述、M2.5/M16 的 styles.rs）为历史记录，
> 当前实现以 [ARCHITECTURE.md](ARCHITECTURE.md) 与 [TAURI_MIGRATION.md](TAURI_MIGRATION.md) 为准。