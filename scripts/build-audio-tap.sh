#!/usr/bin/env bash
# 编译系统声音采集辅助程序 audio-tap：
#   target/audio-tap                                  —— 命令行 / 开发版 App 使用
#   src-tauri/binaries/audio-tap-<target-triple>      —— Tauri sidecar
set -euo pipefail
cd "$(dirname "$0")/.."

TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
mkdir -p target src-tauri/binaries
swiftc -O -swift-version 5 \
  -target arm64-apple-macos14.2 \
  -o target/audio-tap \
  native/macos-audio-tap/main.swift
cp target/audio-tap "src-tauri/binaries/audio-tap-${TRIPLE}"

echo "built: target/audio-tap, src-tauri/binaries/audio-tap-${TRIPLE}"
