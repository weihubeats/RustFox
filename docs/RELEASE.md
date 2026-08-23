# 版本升级与发版流程

RustFox 发版分两步：把版本号从 `X.Y.Z` 升到 `X.Y.(Z+1)`，再用 `vX.Y.(Z+1)` tag 触发 GitHub Actions 产出各平台安装包与 updater 清单。

> **推荐用发版助手脚本**（零依赖，Node ≥ 20）：`scripts/release.mjs`。

## ⭐ 一条命令发版

```bash
node scripts/release.mjs release patch          # 或 minor / major / X.Y.Z
```

自动串联完整链路：

1. **bump**：同步 `tauri.conf.json` / `package.json` / `Cargo.toml` + package-lock；
2. **本地检查**：`lint` → `test` → `build`（任一失败即中止，保留改动供修复；`--skip-checks` 可跳过）；
3. **提交推送**：`chore(release): vX.Y.Z` 推送当前分支；
4. **打标签**：推送 `v{version}` tag，触发多平台构建。

追加 `--publish` 时，脚本会等待构建完成并自动把 Draft Release 转正（需本机安装并登录 `gh` CLI）：

```bash
node scripts/release.mjs release minor --publish
```

安全防护：工作区不干净直接拒绝执行；CI 侧另有「tag 与版本一致」门禁双保险。

## 分步执行（等价手动流）

<details>
<summary>展开：bump / check / tag 子命令与手工等价操作</summary>

### bump —— 仅升级并同步版本文件

```bash
node scripts/release.mjs bump <X.Y.Z | patch | minor | major>
```

自动更新 `frontend/src-tauri/tauri.conf.json`、`frontend/package.json`、`frontend/src-tauri/Cargo.toml`（仅包自身 version 行），并在 `frontend/` 执行 `npm install --package-lock-only` 同步 lockfile。

- updater 以 `tauri.conf.json` 的 `version` 作为当前客户端版本；
- GitHub Actions 打包时读取同处版本号，写入产物名与 `latest.json`。

### check —— 校验三处一致

```bash
node scripts/release.mjs check
```

### tag —— 校验后打标签触发构建

```bash
node scripts/release.mjs tag --push
```

打标前两道防护：三处版本必须一致；工作区必须 clean。
tag 匹配 `v*` 触发 `release.yml`，CI 门禁会再复核一次。

### 手工等价操作（不用脚本时）

```bash
# 1) 手改三处：frontend/src-tauri/tauri.conf.json、frontend/package.json、
#    frontend/src-tauri/Cargo.toml（仅包自身 version 行）
# 2) 同步 lockfile：
cd frontend && npm install --package-lock-only && cd ..
# 3) 提交推送后打 tag：
git commit -am "chore: bump version to X.Y.Z" && git push
git tag vX.Y.Z && git push origin vX.Y.Z
```

</details>

## 三、验证 CI 产出

`release.yml` 会在各 OS 上跑 `tauri build`（Linux / macOS ARM / macOS Intel / Windows），汇总产物到 `release` job 生成 `latest.json`，最终创建 **Draft Release**。

- **Draft 必须手动点 Publish**，否则 `releases/latest/download/latest.json` 返回 404，updater 拿不到新版本；
- 产物列表：macOS `.dmg` + `.app.tar.gz(.sig)`、Linux `.AppImage`、Windows `.exe` + `.msi`，外加 `latest.json`。

### 验证产物版本

打开 `https://github.com/{owner}/{repo}/releases/latest/download/latest.json`：

```json
{
  "version": "X.Y.Z",
  "notes": "...",
  "pub_date": "...",
  "platforms": {
    "darwin-aarch64": { "url": "...", "signature": "..." },
    "linux-x86_64":   { "url": "...", "signature": "..." }
  }
}
```

`version` 必须是你发的那个 `X.Y.Z`（不是 `main`）。

### 客户端验证

应用内「关于 → Check for Updates」，应弹出对应版本更新提示。

## 四、踩坑清单

- **版本号必须是合法 semver**。`latest.json` 里 `version` 字段会被 `semver` crate 解析，`"main"` 之类的值会直接报 `unexpected character` 错误（曾发生在 `workflow_dispatch` 手动触发、未传 tag 参数的场景）。
- **tag 必须指向 bump 后的 commit**。先打 tag 再 bump → tag 指老 commit，发版后 tag 与 `main` HEAD 不一致。
- **不要手动 dispatch `workflow_dispatch` 不传 tag**。`GITHUB_REF_NAME` 在 dispatch 时是分支名 `main`，会被误当版本写入 `latest.json`。
- **未配置 `TAURI_SIGNING_PRIVATE_KEY` secret 前不要打 tag**。签名失败会导致各平台 tar.gz / AppImage / exe 缺一或无 `.sig`，`latest.json` 不完整。
- **Draft Release 不发出去 = 发不了版**。CI 只创建 draft，最后一步 Publish 必须由人点（安全网，避免草稿也走签名流量）。

## 五、命令速查

```bash
# ⭐ 一条命令发版（升版本 → 检查 → 提交推送 → 打标签）
node scripts/release.mjs release patch

# 自动等待构建完成并发布 Draft（需 gh CLI）
node scripts/release.mjs release minor --publish

# 仅同步版本文件 / 校验 / 单独打标
node scripts/release.mjs bump patch
node scripts/release.mjs check
node scripts/release.mjs tag --push
```