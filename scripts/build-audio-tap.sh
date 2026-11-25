#!/usr/bin/env bash
# 编译系统声音采集辅助程序 audio-tap，输出到 target/audio-tap
set -euo pipefail
cd "$(dirname "$0")/.."

mkdir -p target
swiftc -O -swift-version 5 \
  -target arm64-apple-macos14.2 \
  -o target/audio-tap \
  native/macos-audio-tap/main.swift

echo "built: target/audio-tap"
