# AGENTS.md

RustFox：Tauri 2 + Vue 3 的跨平台 API 调试工具（Rust 工作区 + 前端工作区 + 两个独立的 Tauri 工作区）。

## 发布流程

当用户要求「发布 / 发版 / release / 打 tag」时，按照 `.agents/skills/release/SKILL.md` 中定义的流程执行
（该文件同时被 `.opencode/skills/release` 与 `.claude/skills/release` 符号链接共享，修改只需改这一份）。

要点：先确认 main 分支 CI 为 success；用 `node scripts/release.mjs release patch|minor|major|X.Y.Z`
一键升版 + 门禁 + 提交推送 + 打 tag；随后用 GitHub API 轮询 Release workflow；产物为 draft release，需最后转正。
