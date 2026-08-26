#!/usr/bin/env bash
# 确保 tauri externalBin 所需的 rustfox-mcp 侧载二进制存在且为最新。
#
# Tauri 2 的 bundle.externalBin 要求文件名带目标三元组后缀：
#   frontend/src-tauri/binaries/rustfox-mcp-<target-triple>[.exe]
# 本脚本按当前 rustc host 三元组，从根 workspace 构建产物复制过去；
# 目标已存在且比源新（或源未变化）时跳过，增量场景近乎零开销。
#
# 调用方：scripts/package-tauri.sh、frontend package.json 的 "tauri" 命令。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/target/release/rustfox-mcp"
BINARIES_DIR="$ROOT/frontend/src-tauri/binaries"

TRIPLE="$(rustc -vV | awk '/^host:/ {print $2}')"
if [[ "$TRIPLE" == *windows* ]]; then
  DEST="$BINARIES_DIR/rustfox-mcp-$TRIPLE.exe"
else
  DEST="$BINARIES_DIR/rustfox-mcp-$TRIPLE"
fi

need_build=0
if [[ ! -f "$SRC" || "$SRC" -nt "$DEST" ]]; then
  need_build=1
fi

if [[ $need_build -eq 1 ]]; then
  echo "==> 构建 rustfox-mcp（侧载二进制，host=${TRIPLE}）"
  (cd "$ROOT" && cargo build --release -p fox-mcp)
fi

mkdir -p "$BINARIES_DIR"
if [[ ! -f "$DEST" ]] || [[ "$SRC" -nt "$DEST" ]]; then
  cp "$SRC" "$DEST"
  echo "==> 已更新侧载二进制：$DEST"
else
  echo "==> 侧载二进制已是最新：$DEST"
fi
