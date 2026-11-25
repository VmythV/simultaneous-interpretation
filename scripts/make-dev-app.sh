#!/usr/bin/env bash
# 把 tingyi-cli 和 audio-tap 打包成 target/TingYi Dev.app，用来验证 macOS 权限
# （麦克风、系统录音）在"真正的 App"里能否正常申请。
#
# 用法：
#   scripts/make-dev-app.sh                 # 构建并打包
#   scripts/run-dev-app.sh <tingyi-cli 参数> # 以 App 身份运行（见该脚本）
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="${PROFILE:-debug}"
APP="target/TingYi Dev.app"
SIGN_ID="${TINGYI_SIGN_ID:--}"   # 默认临时签名；有自签名证书时设置为证书名称

if [[ "$PROFILE" == "release" ]]; then cargo build --release -p tingyi-cli; else cargo build -p tingyi-cli; fi
scripts/build-audio-tap.sh

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "target/$PROFILE/tingyi-cli" "$APP/Contents/MacOS/tingyi-cli"
cp target/audio-tap "$APP/Contents/MacOS/audio-tap"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>app.tingyi.dev</string>
  <key>CFBundleName</key><string>TingYi Dev</string>
  <key>CFBundleDisplayName</key><string>听译（开发版）</string>
  <key>CFBundleExecutable</key><string>tingyi-cli</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>14.2</string>
  <key>LSUIElement</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>听译需要使用麦克风，把你说的话实时转成文字并翻译。</string>
  <key>NSAudioCaptureUsageDescription</key><string>听译需要录制电脑正在播放的声音，用于实时转文字和翻译。</string>
</dict>
</plist>
PLIST

# 先签内部的辅助程序，再签整个 App
codesign --force --sign "$SIGN_ID" --identifier app.tingyi.dev.audio-tap "$APP/Contents/MacOS/audio-tap"
codesign --force --sign "$SIGN_ID" "$APP"
codesign --verify --verbose=1 "$APP"
echo "built: ${APP}（签名：${SIGN_ID}）"
