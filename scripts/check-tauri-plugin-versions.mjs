#!/usr/bin/env node
/**
 * Tauri 插件版本一致性检查（CI 门禁，零依赖）。
 *
 * 复刻 tauri-cli 的 `check_mismatched_packages`
 * （crates/tauri-cli/src/info/plugins.rs）：成对比较 npm 包与 Rust crate 的
 * **major.minor**，不一致即报错：
 *
 *   tauri                    ↔ @tauri-apps/api
 *   tauri-plugin-<x>         ↔ @tauri-apps/plugin-<x>
 *
 * 真实案例：dependabot 分别升了 `@tauri-apps/plugin-dialog`(npm) 与
 * `tauri-plugin-opener`(Rust)，CI（仅 cargo check）全绿，但 `tauri build`
 * 启动时的该校验直接 exit 1，四个平台打包全部失败。
 *
 * 本脚本只读 lockfile / node_modules，不需要编译，秒级完成。
 *
 * 用法：node scripts/check-tauri-plugin-versions.mjs
 * 退出码：0 一致；1 存在不匹配（并打印明细）。
 */

import { readFileSync, existsSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const FRONTEND = path.join(ROOT, 'frontend')
const TAURI_DIR = path.join(FRONTEND, 'src-tauri')

const readJson = (p) => JSON.parse(readFileSync(p, 'utf8'))
const majorMinor = (v) => {
  const m = /^(\d+)\.(\d+)/.exec(String(v))
  return m ? `${m[1]}.${m[2]}` : null
}

/** npm 已安装版本：优先 package-lock 的 resolved，其次 node_modules。 */
function npmVersion(name) {
  const lockPath = path.join(FRONTEND, 'package-lock.json')
  if (existsSync(lockPath)) {
    const entry = readJson(lockPath).packages?.[`node_modules/${name}`]
    if (entry?.version) return entry.version
  }
  const nm = path.join(FRONTEND, 'node_modules', name, 'package.json')
  if (existsSync(nm)) return readJson(nm).version
  return null
}

/** src-tauri/Cargo.lock 里所有 crate 的解析版本。 */
function rustVersions() {
  const lockPath = path.join(TAURI_DIR, 'Cargo.lock')
  const map = new Map()
  for (const block of readFileSync(lockPath, 'utf8').split('[[package]]')) {
    const name = /name = "([^"]+)"/.exec(block)?.[1]
    const version = /version = "([^"]+)"/.exec(block)?.[1]
    if (name && version) map.set(name, version)
  }
  return map
}

function main() {
  const pkg = readJson(path.join(FRONTEND, 'package.json'))
  const deps = { ...pkg.dependencies, ...pkg.devDependencies }
  const rust = rustVersions()

  // 组装成对清单：npm 名 → rust crate 名
  const pairs = [['tauri', '@tauri-apps/api']]
  for (const name of Object.keys(deps)) {
    const m = /^@tauri-apps\/plugin-(.+)$/.exec(name)
    if (m) pairs.push([`tauri-plugin-${m[1]}`, name])
  }

  const mismatched = []
  const checked = []

  for (const [crateName, npmName] of pairs) {
    const crateVersion = rust.get(crateName)
    const npmVer = npmVersion(npmName)
    // tauri-cli 只在「两侧都存在」时才比较，缺任一侧即跳过。
    if (!crateVersion || !npmVer) continue

    const crateMM = majorMinor(crateVersion)
    const npmMM = majorMinor(npmVer)
    checked.push(`${crateName} (v${crateVersion}) : ${npmName} (v${npmVer})`)
    if (crateMM !== npmMM) {
      mismatched.push(
        `  ✗ ${crateName} (v${crateVersion}) : ${npmName} (v${npmVer})` +
          `  [Rust ${crateMM} ≠ npm ${npmMM}]`,
      )
    }
  }

  if (mismatched.length > 0) {
    console.error(
      '✗ Tauri npm/Rust 插件版本不匹配（major.minor 必须一致，否则 tauri build 会失败）：\n' +
        mismatched.join('\n') +
        '\n\n对齐方式：把 Rust crate 与 npm 包升到同 major.minor（改 package.json / Cargo.lock）。',
    )
    process.exitCode = 1
    return
  }

  console.log(`✓ Tauri npm/Rust 插件版本一致（已校验 ${checked.length} 对）：`)
  for (const line of checked) console.log(`  ${line}`)
}

main()
