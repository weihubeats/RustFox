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

## 前端 UI 规范（实证踩坑）

- **禁止 `:global(html[data-theme=…])` 做主题覆盖**：该写法经 Vite 构建后丢失
  （产物无对应规则，已实证：侧边栏/右键菜单浅色主题因此静默全黑）。
  主题差异一律用 `style.css` 的 CSS 变量表达（深/浅各一套），回归测试
  `frontend/src/themePattern.spec.ts` 会扫全仓 `.vue` 拦截。
- **深/浅验证看构建产物，不只看源码**：`npm run build` 后 grep `dist/assets/*.css`
  确认规则存在（如 `.rf-sidebar` 是否跟变量），dev 与构建行为可能分叉。
- **共享类禁止只定义在某组件 scoped 内**：`m-select-*` 曾只在 EndpointEditor 局部，
  另 4 处引用实际裸奔。跨组件复用的类一律放 `style.css` 全局。
- **方法色 single source**：`@theme` 的 `method-*` 令牌（色值）+ `utils/methodTone.ts`
  （组合：徽章式/纯文本式）。**禁止拼接构造类名**（如 `` `text-method-${x}` ``，
  Tailwind 扫描不到）；新增方法先补映射表，未知方法兜底中性灰。
- **Tailwind 是 v4 不是 v3**：CSS-first，无 config 文件；跳过 preflight；
  `dark:` 变体绑定 `<html data-theme>`（跟主题 Store，不跟系统）。
- **浅色可见性**：`rgba(255,255,255,…)` 边框/底色在浅色下隐形，一律用变量
  （`--border`/`--bg-hover` 等）；彩色底上的白色高光、CodeMirror JS 主题字面量除外。
- **圆角/字号走 token**：`--radius-sm/md/lg/xl`（4/6/8/10/16），碎值（7/5/3px）能收则收；
  饱和底（紫 pill）上的徽章/文字要提亮底色保可读。
- **`prefers-reduced-motion`**：`style.css` 有全局兜底一刀切；JS 计时器类动画
  各组件自理。新增持续动画时检查该媒体查询。
- **EndpointEditor 单实例常驻**：在途/响应/计时状态必须按接口 id 分桶，
  否则切 Tab 会串响应；全局 `sending` 布尔会阻塞多标签并发。

