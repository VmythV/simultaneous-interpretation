# 听译（TingYi）

实时语音转文字 + 翻译的 macOS 桌面应用。选择"听什么"（麦克风 / 电脑声音 / 会议），说的话实时变成文字，并同步翻译成目标语言。

主要功能：

- 边说边出字，一句说完约 1 秒出译文；自动判断翻译方向（中→英、英→中）
- 会议模式：同时听自己和对方，标出"我 / 对方"；系统回声消除 + 文字去重，外放也不会把对方的话录成"我"说的
- 只听指定 App 的声音（比如只听腾讯会议或浏览器），其他软件的提示音、音乐不会被识别
- 悬浮字幕：置顶的透明字幕条，可拖动、可鼠标穿透
- 菜单栏图标、全局快捷键（默认 ⌃⌥S 开始/停止，⌃⌥Z 字幕，⌃⌥X 鼠标穿透）
- 历史记录，导出 TXT / Markdown / SRT

- 方案：[docs/PLAN.md](docs/PLAN.md)
- M0 技术验证记录与实测延迟：[docs/M0.md](docs/M0.md)
- M2 记录（会议模式、字幕、快捷键、历史）：[docs/M2.md](docs/M2.md)
- M3 记录（只听指定 App、会议模式回声消除）：[docs/M3.md](docs/M3.md)

## 安装

```bash
npm install
npm run app:build
```

产物：

- `target/release/bundle/macos/听译.app`：拖进「应用程序」即可使用
- `target/release/bundle/dmg/听译_0.1.0_aarch64.dmg`

首次打开会进入设置向导：

1. 填写语音识别服务的 API Key。推荐阿里云百炼，填一个 Key 就够，识别和翻译共用。
2. 确认翻译服务。
3. 选择默认的音源和目标语言。

说明：

- 第一次使用「麦克风」或「电脑声音」时，系统会弹出授权，点「允许」即可。
- 设置（含 API Key）保存在 `~/Library/Application Support/app.tingyi.desktop/settings.json`，文件权限 600。
- 历史记录保存在同一目录的 `history/` 下，可在设置里关闭。
- 关闭主窗口后，软件会继续在菜单栏运行。要退出，用菜单栏图标里的「退出听译」或按 ⌘Q。
- 默认使用临时签名（ad-hoc），每次重新打包后，系统可能会要求重新授权麦克风和录音权限。想避免的话，先运行一次 `npm run signing-cert` 创建本地证书（需要输入登录密码），之后 `npm run app:build` 会自动用它签名。

## 开发

```bash
npm run app:dev      # 开发模式（热更新）
cargo test           # 核心库测试（含模拟百炼服务、端到端流水线）
npm run typecheck    # 前端类型检查
```

开发模式下，程序由终端启动，权限归属于终端，所以抓不到「电脑声音」。要测试电脑声音，请用打包后的 App，或者用 `scripts/run-dev-app.sh`（见 docs/M0.md）。

## 目录

```
crates/core     核心库：采集、VAD、识别、翻译、流水线（不依赖界面）
crates/cli      命令行验证工具
native/         macOS 电脑声音采集辅助程序（Swift）
src-tauri/      桌面应用外壳：设置、会话、命令
src/            界面（React）
```
