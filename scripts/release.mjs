#!/usr/bin/env node
/**
 * RustFox 发版助手（零依赖，Node >= 20）。
 *
 * 版本号单一事实源：frontend/src-tauri/tauri.conf.json（updater 以它为当前客户端版本）。
 * 本脚本把它同步到其余位置，消除手工改多处配置的负担：
 *
 *   1. frontend/src-tauri/tauri.conf.json
 *   2. frontend/package.json            （+ package-lock.json 经 npm 同步）
 *   3. frontend/src-tauri/Cargo.toml    （仅 rustfox 包自身的 version 行）
 *
 * 用法：
 *   node scripts/release.mjs release patch [--skip-checks] [--publish]   # ⭐ 一条命令发版
 *   node scripts/release.mjs bump <X.Y.Z | patch | minor | major>        # 仅同步版本文件
 *   node scripts/release.mjs check                                       # 校验各处版本一致
 *   node scripts/release.mjs tag [--push]                                # 校验后打 v{version} 标签
 *
 * 发版完整流程（release 子命令已自动串联 ①②③④）：
 *   node scripts/release.mjs release patch
 *     ① bump：同步三处版本文件 + package-lock
 *     ② 本地检查：lint / test / build（--skip-checks 可跳过）
 *     ③ 提交并推送当前分支（chore(release): vX.Y.Z）
 *     ④ 打 v{version} 标签并推送 → 触发 GitHub Actions 多平台构建
 *   加 --publish 时：轮询等待构建完成后自动把 Draft Release 转正（需本机安装并登录 gh）。
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { execSync } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

/** 相对仓库根的三个版本文件路径。 */
export const VERSION_FILES = {
  conf: 'frontend/src-tauri/tauri.conf.json',
  pkg: 'frontend/package.json',
  cargo: 'frontend/src-tauri/Cargo.toml',
}

// ---------- 纯函数（可单测） ----------

/** semver 校验：X.Y.Z（宽松：每段 ≥0 的数字）。 */
export function isValidSemver(v) {
  return /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(v)
}

/**
 * 解析目标版本：显式 X.Y.Z 直接返回；patch/minor/major 基于当前版本递增。
 * 非法输入抛错。
 */
export function resolveTargetVersion(current, arg) {
  if (!arg) throw new Error('缺少目标版本参数（X.Y.Z 或 patch/minor/major）')
  if (isValidSemver(arg)) return arg
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(current)
  if (!m) throw new Error(`当前版本非法：${current}`)
  let [maj, min, pat] = [Number(m[1]), Number(m[2]), Number(m[3])]
  switch (arg) {
    case 'patch':
      pat += 1
      break
    case 'minor':
      min += 1
      pat = 0
      break
    case 'major':
      maj += 1
      min = 0
      pat = 0
      break
    default:
      throw new Error(`无法识别的版本参数：${arg}（可用 X.Y.Z / patch / minor / major）`)
  }
  return `${maj}.${min}.${pat}`
}

/** 读 JSON 文件的 version 字段。 */
export function readJsonVersion(file) {
  return JSON.parse(readFileSync(file, 'utf8')).version
}

/**
 * 把 Cargo.toml 里「包自身」的 version 行替换为指定值。
 * 只匹配文件中第一条顶层 `version = "…"`（依赖版本带缩进或形如 name = { version = … }，不受影响）。
 */
export function applyCargoVersion(content, version) {
  let replaced = false
  const out = content.split('\n').map((line) => {
    if (!replaced && /^version\s*=\s*"[^"]*"\s*$/.test(line)) {
      replaced = true
      return `version = "${version}"`
    }
    return line
  })
  if (!replaced) throw new Error('Cargo.toml 中未找到包自身 version 行')
  return out.join('\n')
}

/**
 * 同步全部版本文件到 target；返回实际写入的文件列表（相对路径）。
 * lockfile 由调用方执行 `npm install --package-lock-only` 处理。
 */
export function syncVersions(root, target) {
  if (!isValidSemver(target)) throw new Error(`非法目标版本：${target}`)

  const confPath = path.join(root, VERSION_FILES.conf)
  const pkgPath = path.join(root, VERSION_FILES.pkg)
  const cargoPath = path.join(root, VERSION_FILES.cargo)

  const conf = JSON.parse(readFileSync(confPath, 'utf8'))
  conf.version = target
  writeFileSync(confPath, JSON.stringify(conf, null, 2) + '\n')

  const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'))
  pkg.version = target
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n')

  writeFileSync(cargoPath, applyCargoVersion(readFileSync(cargoPath, 'utf8'), target))

  return Object.values(VERSION_FILES)
}

/** 一致性检查：返回各处版本与不一致项列表。 */
export function checkConsistency(root) {
  const versions = {
    'tauri.conf.json': readJsonVersion(path.join(root, VERSION_FILES.conf)),
    'package.json': readJsonVersion(path.join(root, VERSION_FILES.pkg)),
    'Cargo.toml': (() => {
      const m = /^version\s*=\s*"([^"]+)"/m.exec(
        readFileSync(path.join(root, VERSION_FILES.cargo), 'utf8'),
      )
      if (!m) throw new Error('Cargo.toml 缺少包版本行')
      return m[1]
    })(),
  }
  const values = Object.values(versions)
  const ok = values.every((v) => v === values[0])
  return { ok, base: values[0], versions }
}

// ---------- CLI ----------

function sh(cmd, opts = {}) {
  execSync(cmd, { stdio: 'inherit', cwd: ROOT, ...opts })
}

function gitOut(args) {
  return execSync(`git ${args}`, { cwd: ROOT, encoding: 'utf8' }).trim()
}

/** 本地发布前检查：lint / test / build（任一失败抛错中止）。 */
function runChecks() {
  const checks = [
    ['lint', 'npm run lint'],
    ['test', 'npm test'],
    ['build', 'npm run build'],
  ]
  for (const [name, cmd] of checks) {
    console.log(`\n▶ 检查 ${name}…`)
    sh(cmd, { cwd: path.join(ROOT, 'frontend') })
    console.log(`✓ ${name} 通过`)
  }
}

/**
 * 等待 tag 触发的 Release 构建结束并把 Draft 转正（需 gh CLI 已登录）。
 * 轮询上限约 40 分钟；gh 缺失时仅打印提示不阻塞。
 */
function watchAndPublish(tag) {
  let gh
  try {
    gh = execSync('command -v gh', { encoding: 'utf8' }).trim()
  } catch {
    gh = ''
  }
  if (!gh) {
    console.log('\nℹ 未检测到 gh CLI：构建完成后请到 GitHub Release 页手动点 Publish Draft')
    return
  }
  console.log(`\n⏳ 等待 ${tag} 的多平台构建完成（最长 40 分钟）…`)
  const deadline = Date.now() + 40 * 60 * 1000
  let runId = null
  while (Date.now() < deadline) {
    try {
      const out = execSync(
        `gh run list --workflow=release.yml --limit 10 --json databaseId,headBranch,status,event`,
        { cwd: ROOT, encoding: 'utf8' },
      )
      const runs = JSON.parse(out).filter((r) => r.headBranch === tag && r.event === 'push')
      if (runs.length && runs[0].status === 'completed') break
      if (runs.length) runId = runs[0].databaseId
    } catch {
      /* gh 偶发网络错误：忽略本轮 */
    }
    execSync('sleep 30')
  }
  if (!runId) {
    console.log('ℹ 未匹配到该 tag 的运行记录，稍后请手动检查并发布 Draft')
    return
  }
  sh(`gh run watch ${runId} --exit-status`)
  // watch 成功（exit 0）后把草稿转正；失败则保留草稿供排查
  try {
    sh(`gh release edit ${tag} --draft=false`)
    console.log(`\n🎉 ${tag} 已正式发布，updater 将在客户端轮询时收到更新`)
  } catch {
    console.error('✗ 发布转正失败，请到 Release 页检查 Draft 状态')
    process.exitCode = 1
  }
}

/** 一条命令发版：bump → checks → commit/push → tag/push（→ 可选 publish）。 */
function releaseFlow(arg, flags) {
  if (gitOut('status --porcelain')) {
    console.error('✗ 工作区有未提交改动，请先处理干净再发版：')
    console.error(gitOut('status --porcelain'))
    process.exitCode = 1
    return
  }
  const branch = gitOut('rev-parse --abbrev-ref HEAD')
  const current = readJsonVersion(path.join(ROOT, VERSION_FILES.conf))
  const target = resolveTargetVersion(current, arg)
  const tag = `v${target}`

  // ① 版本同步
  syncVersions(ROOT, target)
  sh('npm install --package-lock-only', { cwd: path.join(ROOT, 'frontend') })
  console.log(`① 版本已同步：${current} → ${target}`)

  // ② 本地检查（失败则保留改动供修复，不发版）
  if (flags.has('--skip-checks')) {
    console.log('② 跳过本地检查（--skip-checks）')
  } else {
    try {
      runChecks()
    } catch {
      console.error('\n✗ 本地检查未通过，已停止发版。修复后重跑本命令即可。')
      process.exitCode = 1
      return
    }
  }

  // ③ 提交 + 推送分支
  sh('git add -A')
  sh(`git commit -m "chore(release): ${tag}"`)
  sh('git push')
  console.log(`③ 已提交并推送 ${branch}`)

  // ④ 打标签 + 推送（触发多平台构建）
  sh(`git tag -a ${tag} -m "RustFox ${tag}"`)
  sh(`git push origin ${tag}`)
  console.log(`④ 标签 ${tag} 已推送，GitHub Actions 开始构建`)

  if (flags.has('--publish')) {
    watchAndPublish(tag)
  } else {
    console.log('\n下一步：构建完成后到 GitHub Release 页将 Draft 点 Publish；')
    console.log('或使用 node scripts/release.mjs release <version> --publish 自动完成。')
  }
}

function main() {
  const [cmd, ...rest] = process.argv.slice(2)
  switch (cmd) {
    case 'release': {
      const flags = new Set(rest.filter((a) => a.startsWith('--')))
      const arg = rest.find((a) => !a.startsWith('--'))
      releaseFlow(arg, flags)
      break
    }
    case 'bump': {
      const current = readJsonVersion(path.join(ROOT, VERSION_FILES.conf))
      const target = resolveTargetVersion(current, rest[0])
      syncVersions(ROOT, target)
      sh('npm install --package-lock-only', { cwd: path.join(ROOT, 'frontend') })
      console.log(`\n✓ 版本已同步：${current} → ${target}`)
      console.log('下一步：')
      console.log('  git commit -am "chore: bump v' + target + '" && git push')
      console.log('  node scripts/release.mjs tag --push\n')
      break
    }
    case 'check': {
      const { ok, base, versions } = checkConsistency(ROOT)
      for (const [file, v] of Object.entries(versions)) {
        console.log(`${ok || v === base ? '✓' : '✗'} ${file.padEnd(18)} ${v}`)
      }
      if (!ok) {
        console.error('\n✗ 版本不一致，请运行 node scripts/release.mjs bump <version> 统一')
        process.exitCode = 1
      }
      break
    }
    case 'tag': {
      const push = rest.includes('--push')
      const { ok, base, versions } = checkConsistency(ROOT)
      if (!ok) {
        console.error('✗ 各处版本不一致，先执行 bump：', JSON.stringify(versions))
        process.exitCode = 1
        break
      }
      const dirty = execSync('git status --porcelain', { cwd: ROOT, encoding: 'utf8' }).trim()
      if (dirty) {
        console.error('✗ 工作区有未提交改动，请先 commit（避免 tag 与产物内容不符）：')
        console.error(dirty)
        process.exitCode = 1
        break
      }
      const tag = `v${base}`
      sh(`git tag -a ${tag} -m "RustFox ${tag}"`)
      console.log(`✓ 已创建标签 ${tag}`)
      if (push) {
        sh(`git push origin ${tag}`)
        console.log(`✓ 已推送 ${tag} —— GitHub Actions 开始构建，完成后到 Release 页发布 Draft`)
      } else {
        console.log('推送到远端以触发构建：git push origin ' + tag)
      }
      break
    }
    default:
      console.log(
        [
          '用法：node scripts/release.mjs <命令>',
          '',
          '  ⭐ release <X.Y.Z|patch|minor|major> [--skip-checks] [--publish]',
          '       一条命令发版：升版本 → 本地检查 → 提交推送 → 打标签触发构建',
          '       --publish 额外等待构建完成并自动发布 Draft（需 gh CLI）',
          '  bump <X.Y.Z|patch|minor|major>   仅升级并同步全部版本文件（含 package-lock）',
          '  check                            校验三处版本一致',
          '  tag [--push]                     校验一致且工作区干净后打 v{version} 标签',
        ].join('\n'),
      )
      if (cmd !== undefined) process.exitCode = 1
  }
}

// 被 import（单测）时不执行 CLI
if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  main()
}
