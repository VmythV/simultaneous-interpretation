#!/usr/bin/env bash
# 以 "TingYi Dev.app" 的身份运行 tingyi-cli（权限归属于这个 App，而不是终端）。
# 输出写到 target/dev-app.log，并同步显示在终端。
#
# 例：
#   scripts/run-dev-app.sh perm --request
#   scripts/run-dev-app.sh capture --source both --secs 10 --out target/cap
#   scripts/run-dev-app.sh run --source system --secs 60
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
APP="$ROOT/target/TingYi Dev.app"
LOG="$ROOT/target/dev-app.log"

[[ -d "$APP" ]] || scripts/make-dev-app.sh

# 相对路径参数转成绝对路径（App 启动时工作目录是 /）
args=()
for a in "$@"; do
  if [[ "$a" == target/* || "$a" == ./* ]]; then args+=("$ROOT/${a#./}"); else args+=("$a"); fi
done

: > "$LOG"
tail -n +1 -f "$LOG" &
TAIL=$!
trap 'kill $TAIL 2>/dev/null' EXIT
open -n -W --stdout "$LOG" --stderr "$LOG" "$APP" --args --env-file "$ROOT/.env" "${args[@]}"
sleep 0.3
