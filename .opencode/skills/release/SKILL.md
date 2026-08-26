---
name: release
description: 发布 RustFox 新版本：升级版本号、提交推送、打 tag 触发多平台构建。当用户说「发布」「发版」「release」「打个 tag」「升级版本发布」时使用。仅用于本仓库（weihubeats/RustFox）的发版流程。
---

# RustFox 发布流程

用户说「发布」（可带版本参数：`发布 minor`、`发布 0.1.0`；缺省 patch）时执行以下流程。

## 0. 前置检查（任何一步失败即停下询问）

```bash
git status --short                 # 有未提交改动 → 先按规范提交推送
curl -s "https://api.github.com/repos/weihubeats/RustFox/actions/runs?branch=main&per_page=1" \
  | python3 -c "import json,sys;r=json.load(sys.stdin)['workflow_runs'][0];print(r['conclusion'])"
```

- main 最新 CI 必须为 `success`；失败则先修 CI，不带病发版
- 确认远端可达（此前出现过 github.com 连接超时，需用户检查代理）

## 1. 一条命令发版

```bash
node scripts/release.mjs release patch     # 或 minor / major / 显式 X.Y.Z
```

脚本自动完成：三处版本号同步（tauri.conf.json / package.json / src-tauri Cargo.toml +
lockfile）→ lint / test / build 门禁 → `chore(release): vX.Y.Z` 提交推送 → 打 `vX.Y.Z`
标签推送 → 触发 Release workflow。

任一门禁失败：修复后从头重跑，**不要**手动补版本号。

## 2. 监控构建（约 10~25 分钟，四平台并行）

本机可能没有 `gh`，用 GitHub API 轮询（每 2~3 分钟一次，循环直到 completed）：

```bash
curl -s "https://api.github.com/repos/weihubeats/RustFox/actions/runs?event=push&per_page=5" \
  | python3 -c "import json,sys;[print(r['name'],r['head_branch'],r['status'],r['conclusion']) for r in json.load(sys.stdin)['workflow_runs'] if r['name']=='Release']"
```

- `conclusion: success` → 继续
- `failure` → 拉取失败 job 日志定位（`https://api.github.com/repos/weihubeats/RustFox/actions/runs/<id>/jobs`），
  修复后删除远端 tag 重发：`git push origin :refs/tags/vX.Y.Z && git tag -d vX.Y.Z`

## 3. 转正 Draft Release

workflow 产物为 **draft**，需最后一步发布：

- 本机有 `gh` 且已登录：`gh release edit vX.Y.Z --draft=false`
- 没有：把 Releases 页 Draft 链接给用户手动点 Publish，并说明 updater（latest.json）
  会在发布后对已装用户生效

## 4. 汇报

输出：版本号、tag、commit、workflow 耗时、产物清单（4 平台）、Release 链接、是否已转正。

## 注意

- `--publish` 选项可自动轮询转正，但依赖本机 `gh`，默认不用
- 发版前若有未推送的 feature 分支改动，先确认用户意图（发版只针对 main）
- 版本号当前 0.0.x 阶段默认 patch；用户明确说「小版本/大版本」才用 minor/major
