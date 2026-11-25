#!/usr/bin/env bash
# 打包听译（.app + .dmg）。有「TingYi Local Signing」证书时用它签名，否则使用临时签名。
set -euo pipefail
cd "$(dirname "$0")/.."

scripts/build-audio-tap.sh

CERT="TingYi Local Signing"
if security find-identity -p codesigning 2>/dev/null | grep -q "$CERT"; then
  echo "使用证书签名：$CERT"
  export APPLE_SIGNING_IDENTITY="$CERT"
else
  echo "未找到「${CERT}」证书，使用临时签名（每次重新打包后可能需要重新授权麦克风 / 系统录音）"
  echo "如需避免重复授权，先运行一次：scripts/create-signing-cert.sh"
fi

npx tauri build "$@"
