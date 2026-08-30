# AGENTS.md

RustFox：Tauri 2 + Vue 3 的跨平台 API 调试工具（Rust 工作区 + 前端工作区 + 两个独立的 Tauri 工作区）。

## 技能（Skills）

技能正文统一放 `.agents/skills/<name>/SKILL.md`，`.claude/skills/<name>` 为指向它的符号链接：

- `.agents/skills/`：ZCode 原生识别（跨工具标准位置）；
- `.claude/skills/`：Claude Code 原生识别，OpenCode 亦兼容读取——因此**无需**再为
  OpenCode 单独建 `.opencode/skills`；
- 修改技能只改 `.agents` 一份，两个链接同步生效。

技能清单：

- **release**：发布流程——CI success 前置 → `node scripts/release.mjs` 一键升版推送打 tag →
  轮询 Release workflow → draft release 转正。触发词：「发布 / 发版 / release / 打 tag」。
- **push**：推送流程——按 CI 同款门禁预检（根 workspace fmt/check/clippy/test、
  fox-tauri check/test、前端 lint/test/build）→ 主题化提交 → 推送 → 监控 CI 直到绿。
  触发词：「推送 / push / 提交推送」。

