# 听译（TingYi）— 实时语音翻译 方案

> 版本 v0.2 · 2026-11-25 · 状态：待审核

## 1. 一句话定位

打开就能用的桌面同传助手：选好"听什么"（麦克风 / 电脑声音 / 两者同时），说话内容实时变成文字，同时翻译成目标语言。

软件名 **听译**：说的就是它做的事——"听"声音、"译"文字，一眼看懂。

---

## 2. 已确认的需求

| # | 项 | 结论 |
|---|---|---|
| 1 | 音源 | 麦克风、电脑声音（系统正在播放的声音）可切换；也支持两路同时采集（会议模式，标出"我 / 对方"） |
| 2 | 平台 | 先做 macOS（最低 14.2），架构上给 Windows 留路 |
| 3 | 语音识别 | 可插拔：流式识别（第一家：阿里云百炼）+ OpenAI 兼容的分段识别；以后可加本地模型 |
| 4 | 翻译 | 只用大模型（OpenAI 兼容协议），不接传统翻译 API |
| 5 | 主要语言 | 中文为主 |
| 6 | 显示 | 主窗口 + 悬浮字幕 |
| 7 | 历史记录 | 可在设置里选择是否保存到本地 |
| 8 | 分发 | 自用；打成 .dmg，能装在自己的 Mac 上即可（不做苹果公证） |
| 9 | 界面 | 简洁明了，一眼看懂用途；风格简约，操作和交互简单 |
| 10 | 前端 | React |

---

## 3. 设计原则

1. **一眼看懂**：没开始时，主窗口中间直接写"实时语音转文字 + 翻译"，下面用三步说明怎么用。
2. **主界面只有三个控件**：听什么、译成什么、开始/停止。其余设置全部放进设置窗口。
3. **一个主按钮**：开始/停止是唯一的主操作，也能用全局快捷键触发。
4. **配置靠预设**：选服务商 → 粘贴 API Key → 点"测试"。Base URL、模型名自动填好，收在"高级"里。百炼的业务空间专属 Key 走通用域名也能用，不需要额外填地址。
5. **首次启动有向导**：3 步配好，以后打开就能用。
6. **报错说人话**：出错的位置直接显示原因，并给一个解决按钮（比如"API Key 无效 → 去设置"）。
7. **视觉**：跟随系统浅色/深色；用系统字体（苹方）；只用一种强调色；多留白。译文是主角（深色、稍大），原文是配角（灰色、稍小）。

---

## 4. 界面设计

### 4.1 主窗口：未开始

```
 听译                                                        ⚙
 ──────────────────────────────────────────────────────────────

                     实时语音转文字 + 翻译
          ① 选择要听的声音   ② 点「开始」   ③ 实时看原文和译文


        听  [ 麦克风 ▾ ]                    译成  [ 中文 ▾ ]

                          [  ● 开始  ]

```

"听"的下拉选项：

- 麦克风（二级菜单可选具体设备）
- 电脑声音
- 麦克风 + 电脑声音（会议）

### 4.2 主窗口：进行中

```
 听译   ● 电脑声音 ▾   译成 中文 ▾   ▁▃▅▃▁              [ ■ 停止 ]  ⚙
 ──────────────────────────────────────────────────────────────
 So the key idea of this paper is...
 这篇论文的核心思想是……

 We can reduce the latency by half.
 我们可以把延迟降低一半。

 and the second part is▍                      ← 浅灰：正在识别
 ──────────────────────────────────────────────────────────────
 字幕  复制  导出  清空                                      历史
```

- 新内容自动滚到底。用户往上翻时暂停自动滚动，并显示"回到最新"按钮。
- 会议模式下每段前面加一个小标签，用颜色区分：

```
 对方  So the key idea of this paper is...
       这篇论文的核心思想是……
 我    我觉得这个方向很好。
       I think this direction is great.
```

- "历史"入口只在开启"保存历史记录"后显示。

### 4.3 悬浮字幕

```
   ╭──────────────────────────────────────────────╮
     我们可以把延迟降低一半。
     We can reduce the latency by half.
   ╰──────────────────────────────────────────────╯
```

- 半透明深色圆角背景，始终置顶，可以拖动、调大小。
- 只显示最近 1–2 句。译文用大字，原文用小字，也可以设成只显示译文。
- 鼠标穿透：开启后，点击会穿过字幕落到下面的窗口。用快捷键开关。

### 4.4 首次启动向导（3 步）

1. **语音识别**：选服务商（默认"阿里云百炼"）→ 粘贴 API Key → [测试]。测试用软件内置的一段中文示例音频，看能不能识别出来，这一步不需要麦克风权限。
2. **翻译**：选服务商，可以直接选"和语音识别用同一个"，复用同一个 Key → [测试]，翻译一句示例。
3. **默认设置**：默认音源、目标语言、是否保存历史 → 完成。

### 4.5 设置窗口

左侧标签依次是：通用 / 语音识别 / 翻译 / 字幕 / 快捷键。

| 标签 | 常用项（直接显示） | 高级（默认折叠） |
|---|---|---|
| 通用 | 默认音源、麦克风设备、目标语言、打开软件后自动开始、保存历史记录（开关）、清空历史 | 断句停顿时长（默认 500ms，可调 300–1000ms）、单句最长时长、VAD 灵敏度 |
| 语音识别 | 服务商预设、API Key、[测试] | 模式（流式/分段）、Base URL、模型名、识别语言 |
| 翻译 | 翻译开关（关掉就只转文字）、服务商预设、API Key、[测试] | Base URL、模型名、temperature、上下文句数、备用目标语言、自定义提示词、实时预翻译 |
| 字幕 | 字号、透明度、显示原文+译文 / 只显示译文 | 字体、背景色 |
| 快捷键 | 开始/停止、显示/隐藏字幕、鼠标穿透 | — |

### 4.6 菜单栏图标

点开后可以：开始/停止、切换音源、显示/隐藏字幕、打开主窗口、退出。正在听的时候图标会变色。

---

## 5. 技术方案

### 5.1 技术选型

| 层 | 选型 | 说明 |
|---|---|---|
| 应用框架 | Tauri 2 | Rust 后端 + Web 前端，安装包约 10MB，以后能直接出 Windows 版 |
| 前端 | React + TypeScript + Vite + Tailwind CSS + zustand | 组件自己写，保持简约 |
| 麦克风采集 | `cpal` | 跨平台 |
| 电脑声音（macOS） | Core Audio Process Tap（macOS 14.2+），写成一个小的 Swift 辅助程序 | 不用装虚拟声卡，以后还能只抓某个 App 的声音 |
| 电脑声音（Windows，以后） | WASAPI Loopback（`cpal` 已支持） | — |
| 重采样 | `rubato` | 统一转成 16kHz 单声道 16-bit PCM |
| 断句 VAD | Silero VAD（ONNX 模型，用 `ort` 运行） | 判断说话和停顿；没人说话时不上传，省钱，也能防止 Whisper 凭空生成句子 |
| 语种判断 | `whatlang` + 字符类型统计 | 在本地判断原文是什么语言，决定往哪个方向翻译 |
| 网络 | `reqwest`（SSE 流式）+ `tokio-tungstenite`（WebSocket） | — |
| 配置 | JSON 文件 | 放在 `~/Library/Application Support/<bundle id>/` |
| 密钥 | 直接保存在设置文件里 | 自用场景，按用户要求不用钥匙串；设置文件权限设为 600（仅本人可读） |
| 历史记录 | JSON 文件（每次会话一个 + `index.json`） | 只在开启"保存历史记录"时写入；M2 定稿，自用数据量小，不引入数据库 |

### 5.2 数据流

```
[音频源]   麦克风(cpal) / 电脑声音(Swift 辅助程序)；会议模式下两路同时跑
   │
   ▼
[预处理]   每一路：重采样到 16kHz 单声道 → Silero VAD 判断说话 / 停顿
   │
   ▼
[识别]     每一路一个识别会话（流式或分段），输出 partial / final，并带上 channel 标记（我 / 对方）
   │
   ▼
[翻译]     翻译调度器：拿到 final 句子 + 前 3 句上下文 → 大模型流式输出译文
   │
   ▼
[输出]     事件 → 主窗口 / 悬浮字幕 / 本地历史库（可选）
```

各层只通过 channel 和事件通信，所以换服务商、加音源都不会影响界面代码。

### 5.3 语音识别模块

**两种模式**

| 模式 | 工作方式 | 实时感 |
|---|---|---|
| 流式 | 通过 WebSocket 持续推送音频，服务端实时返回临时结果（partial）和整句结果（final） | 边说边出字 |
| 分段 | VAD 检测到停顿后，把整句音频以 WAV 格式发到 `/v1/audio/transcriptions` | 一句说完后 1–2 秒出字 |

分段模式有一个可选开关"准实时预览"：说话过程中每隔约 1.5 秒，把还没说完的那段重新识别一次，用来显示临时文字。代价是识别费用大约翻倍，默认关闭。

**服务商预设**

| 预设 | 模式 | 说明 |
|---|---|---|
| 阿里云百炼（推荐） | 流式 | 默认 `qwen-audio-3.1-asr-flash-streaming`（官方推荐用于实时字幕/会议），可换 `fun-asr-realtime`、`paraformer-realtime-v2`（三者同一套 run-task 协议）；识别模型需在百炼控制台单独开通；同一个 Key 也能用来翻译 |
| 硅基流动 | 分段 | 兼容 OpenAI，有 SenseVoice 等中文效果好的模型 |
| OpenAI | 分段 | gpt-4o-transcribe / whisper-1 |
| 自定义（OpenAI 兼容） | 分段 | 任何兼容 `/v1/audio/transcriptions` 的服务，比如 Groq 或自建的 whisper 服务 |

**省钱：一句话一个识别任务**（M0 定稿）：

- 百炼的一条 WebSocket 连接可以依次跑多个任务。VAD 判断开始说话时发 `run-task`，并补发 300ms 预缓冲音频；说话中的音频按每 100ms 发一帧；VAD 断句后发 `finish-task`，服务端立刻返回这一句的最终结果。
- 静音期间不发音频，所以不计费。
- 连接空闲超过 45 秒（服务端约 60 秒会关闭）就在下次说话时重连；`task-failed` 之后连接作废并重建。

**接口草图**

```rust
pub enum SttMode { Streaming, Segmented }

pub enum SttEvent {
    Partial { utterance_id: u64, text: String },
    Final   { utterance_id: u64, text: String, start_ms: u64, end_ms: u64, lang: Option<String> },
    Error(SttError),
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    fn mode(&self) -> SttMode;
    async fn open(&self, cfg: &SttConfig, events: mpsc::Sender<SttEvent>) -> Result<Box<dyn SttSession>>;
}

#[async_trait]
pub trait SttSession: Send {
    async fn push_audio(&mut self, pcm: &[i16]) -> Result<()>; // 16kHz 单声道
    async fn end_segment(&mut self) -> Result<()>;             // VAD 判定一句结束
    async fn close(self: Box<Self>) -> Result<()>;
}
```

### 5.4 翻译模块

- **协议**：只用 OpenAI 兼容的 `/v1/chat/completions`，开启 `stream: true`。
- **服务商预设**：阿里云百炼、DeepSeek、硅基流动、OpenAI、Ollama（本地）、自定义。百炼默认 `qwen-flash`：支持 system 提示词和上文，限流 3 万次/分钟。`qwen-mt` 系列不适合做默认，因为它不支持 system 消息，只能单轮对话，限流也只有 60 次/分钟。
- **自动判断翻译方向**：
  - 设置里有"目标语言"（默认中文）和"备用语言"（默认 English）。
  - 原文不是目标语言 → 翻成目标语言；原文本来就是目标语言 → 翻成备用语言。
  - 效果：说中文出英文，听英文出中文，会议模式双向都能用，不用手动切换。
  - 语种在本地判断：优先用识别服务返回的语种，没有就用 `whatlang` 加字符统计。
- **什么时候翻译**：默认只翻已经确定的整句，避免译文跳动，也省钱。可选开启"实时预翻译"：临时结果停顿 800ms 后先翻一版，整句确定后再覆盖。
- **上下文**：每次带上前 3 句原文和译文（两路合在一起按时间排序），让术语、人称前后一致。
- **默认提示词**（可在设置里改）：

  ```
  你是专业的同声传译员。把下面这句话翻译成{目标语言}。
  要求：只输出译文；口语化、简洁；专有名词和上文保持一致；不要解释，不要加引号。
  上文（仅供理解，不要翻译）：
  {最近 3 句原文/译文}
  ```

- **并发与容错**：每句的翻译各自独立，翻译慢了不会挡住识别，界面按句子顺序显示。失败自动重试 1 次，还失败就在这一句下面显示"翻译失败 · 重试"。

**接口草图**

```rust
pub struct TranslateRequest {
    pub text: String,
    pub target_lang: String,
    pub context: Vec<(String, String)>, // (原文, 译文)
}

#[async_trait]
pub trait Translator: Send + Sync {
    /// 增量文字通过 delta 推给前端，返回完整译文
    async fn translate(&self, req: TranslateRequest, delta: mpsc::Sender<String>) -> Result<String>;
}
```

### 5.5 会议模式（两路同时）

- 麦克风和电脑声音各跑一条独立的流水线（各自的 VAD 和识别会话），句子分别标为"我"和"对方"。
- **回声问题**：用外放时，麦克风会把对方的声音再录一遍，导致同一句话出现两次。处理办法：
  1. 选会议模式时提示"建议戴耳机"。
  2. 去重：如果"我"的一句和前后 3 秒内"对方"的一句相似度超过 80%，丢掉"我"那一句。
  3. 以后（M3）：用 macOS 系统自带的回声消除（Voice Processing）采集麦克风。

### 5.6 存储

| 内容 | 位置 | 说明 |
|---|---|---|
| 配置 | `settings.json` | 下文有结构示例 |
| API Key | `settings.json` | 识别和翻译各一个字段；选了"和识别用同一个"就共用；文件权限 600 |
| 历史记录 | `history/<开始时间>.json` + `history/index.json` | 只在开启"保存历史"时写入；关闭开关不会删掉已有记录，另有"清空历史"按钮 |

每次"开始 → 停止"算一个会话。导出格式支持 TXT、Markdown 和 SRT（SRT 带时间轴）。

**配置结构示例**

```json
{
  "audio": {
    "defaultSource": "mic",
    "micDeviceId": "default",
    "autoStart": false,
    "silenceMs": 600,
    "maxSegmentSec": 15
  },
  "stt": {
    "preset": "aliyun",
    "mode": "streaming",
    "baseUrl": "",
    "model": "",
    "language": "auto"
  },
  "translate": {
    "enabled": true,
    "preset": "aliyun",
    "shareKeyWithStt": true,
    "baseUrl": "",
    "model": "",
    "temperature": 0.3,
    "targetLang": "zh",
    "fallbackLang": "en",
    "contextSize": 3,
    "prompt": null,
    "translatePartial": false
  },
  "subtitle": { "fontSize": 28, "opacity": 0.85, "show": "both" },
  "history": { "save": true },
  "hotkeys": { "toggle": "Alt+Cmd+S", "subtitle": "Alt+Cmd+H", "clickThrough": "Alt+Cmd+T" }
}
```

**历史记录文件**：每条包含 `id`、`startedAt`、`endedAt`、`source`、`targetLang`，以及 `entries[]`（`id`、`channel`、`text`、`translation`、`startMs`、`endMs`）。

### 5.7 前后端接口

**命令**（前端调后端）

| 命令 | 作用 |
|---|---|
| `start(source)` / `stop()` | 开始 / 停止 |
| `list_input_devices()` | 列出麦克风设备 |
| `get_settings()` / `save_settings()` | 读取 / 保存配置 |
| `test_stt()` / `test_translator()` | 测试连接 |
| `list_sessions()` / `get_session(id)` / `delete_session(id)` / `clear_history()` | 历史记录 |
| `export_session(id, format)` | 导出 |
| `open_privacy_settings(kind)` | 跳转到系统隐私设置 |

**事件**（后端推给前端）

| 事件 | 内容 |
|---|---|
| `pipeline:state` | 空闲 / 连接中 / 正在听 / 出错 |
| `audio:level` | 每一路的实时音量（用于电平条） |
| `stt:partial` / `stt:final` | 识别结果，带 `utteranceId`、`channel` |
| `tr:delta` / `tr:done` / `tr:error` | 译文增量 / 完成 / 失败 |
| `error` | 需要用户处理的错误，比如权限、Key、网络 |

### 5.8 目录结构

M0 起改为 Cargo workspace：核心逻辑放在独立的 `tingyi-core` 库里，命令行 demo 和桌面应用共用。

```
simultaneous-interpretation/
├── Cargo.toml                    # workspace
├── docs/PLAN.md
├── crates/
│   ├── core/                     # tingyi-core：不依赖界面
│   │   └── src/
│   │       ├── audio/            # mic.rs(cpal)、system_macos.rs(调 audio-tap)、resample.rs
│   │       ├── vad.rs            # Silero VAD + 断句状态机
│   │       ├── stt/              # aliyun.rs(流式)、openai.rs(分段)
│   │       ├── translate/        # openai_chat.rs(流式)、lang.rs(语种/方向)
│   │       ├── pipeline.rs       # 音频 → VAD → 识别 → 翻译，输出 PipelineEvent
│   │       └── wav.rs
│   └── cli/                      # tingyi-cli：M0 验证工具
├── native/macos-audio-tap/       # Swift 辅助程序：抓电脑声音
├── resources/silero_vad.onnx     # 编译时嵌入 tingyi-core
├── scripts/                      # 编译辅助程序、打包开发版 App
├── src-tauri/                    # （M1）Tauri 外壳：命令、设置、会话管理
└── src/                          # （M1）React 前端
    ├── windows/                  # main / subtitle / settings / onboarding
    ├── components/
    ├── stores/
    └── lib/ipc.ts
```

**Swift 辅助程序的约定**（M0 定稿）：

- 启动：`audio-tap --watch-stdin`。stdin 被关闭（父进程退出）时，辅助程序自动清理并退出。
- stdout：持续输出 **f32le 单声道 PCM，采样率为系统原生**。重采样统一放在 Rust 端，和麦克风走同一条路径。
- stderr：每行一个 JSON 事件，包括 `format`（采样率）、`started`（带权限状态）、`error`。
- 调试命令：`--meter` 打印音量，`--check` / `--request` 检查或请求"系统录音"权限。
- 由 Rust 端启动和关闭，作为 Tauri sidecar 打进 App。

### 5.9 macOS 权限与打包

- **麦克风**：Info.plist 写 `NSMicrophoneUsageDescription`，第一次使用时系统弹窗。
- **电脑声音**：Info.plist 写 `NSAudioCaptureUsageDescription`。权限在"系统设置 → 隐私与安全性 → 屏幕与系统录音"里（名称以系统实际显示为准）。被拒时软件会显示说明，并给一个"打开系统设置"按钮。
- **签名**：用一张本地自签名的"代码签名"证书。如果只用临时签名（ad-hoc），每次重新打包后 macOS 都会当成一个新 App，要重新授权。
- **打包**：`tauri build` 生成 `.app` 和 `.dmg`，拖进"应用程序"就能用。不做苹果公证：在本机构建的不受影响；拷到别的 Mac 上，第一次要右键 → 打开。
- **悬浮字幕**：透明窗口要开启 Tauri 的 `macOSPrivateApi`。这只会影响上架 App Store，自用没问题。

---

## 6. 开发里程碑与验收

### M0 技术验证（命令行 demo，先把风险最大的几项验证掉）

- [x] Swift 辅助程序抓到电脑声音，并确认打包签名后能正常弹出权限申请
- [x] `cpal` 抓到麦克风；两路同时采集稳定
- [x] Silero VAD 断句准确
- [x] 阿里云百炼流式识别跑通；OpenAI 兼容分段识别跑通（分段只用模拟服务验证过）
- [x] 大模型流式翻译跑通，记录各环节的实际延迟

详细记录见 [M0.md](M0.md)。

**验收**：在命令行里说一句中文，能实时打出原文，并跟着打出英文译文；放一段英文视频，能打出英文原文和中文译文。

### M1 最小可用版本（✅ 已开发完成，待用户验收）

- 搭好 Tauri 应用骨架：主窗口（4.1、4.2）、设置窗口（4.5）、首次启动向导（4.4）
- 服务商预设，Key 存在设置里，测试连接
- 麦克风和电脑声音二选一，可切换
- 识别：百炼流式 + OpenAI 兼容分段
- 翻译：大模型流式输出，自动判断翻译方向
- 复制、清空

**验收**：

- 配置完重启软件，点一下"开始"就能用。
- 中文边说边出字；一句说完后，1.5 秒内开始出现译文（流式模式）。
- 切换音源正常。

### M2 完整体验（✅ 已开发完成，待用户验收；记录见 [M2.md](M2.md)）

- 会议模式（两路同时，"我 / 对方"标签，回声去重）
- 悬浮字幕、菜单栏图标、全局快捷键
- 历史记录（可开关）、导出 TXT / Markdown / SRT
- 生成 `.dmg` 安装包（自签名证书）

**验收**：开视频会议时，悬浮字幕能实时显示双方的翻译；关掉软件再打开，还能在历史里找到刚才的记录并导出。

### M3 增强（这次只做前两项，其余暂缓）

- [x] 只听指定 App 的声音：比如只听腾讯会议或浏览器。按"责任进程"把辅助进程归到对应的 App 名下（Chrome Helper、Safari 的 WebKit 进程等）；新出现的音频进程会被立刻加入
- [x] 会议模式回声消除：用 macOS 自带的 Voice Processing 采集麦克风，同时把它对其他软件的音量压低（ducking）调到最小；文字层面的去重继续保留，作为兜底
- [ ] 术语表（暂缓）
- [ ] 本地识别模型（whisper.cpp / SenseVoice），离线可用（暂缓）
- [ ] Windows 版（暂缓）

记录见 [M3.md](M3.md)。

---

## 7. 延迟目标

| 指标 | 流式（百炼） | 分段（OpenAI 兼容） |
|---|---|---|
| 说话时原文开始出现 | ≤ 0.5 秒 | 不出现（除非开启"准实时预览"） |
| 一句说完到原文确定 | ≤ 0.8 秒 | 1–2 秒 |
| 一句说完到译文开始出现 | ≤ 1.5 秒 | 2–3 秒 |

以上是目标值，M0 会实测，实际值受网络和服务商影响。

---

## 8. 风险与应对

| 风险 | 应对 |
|---|---|
| 系统录音权限不能正常弹出，或重新打包后失效 | M0 先验证；用固定的自签名证书 |
| Whisper 类模型在静音时凭空生成句子（"谢谢观看""字幕由……提供"） | VAD 过滤静音 + 黑名单兜底 |
| 一直说不停顿，迟迟不出字 | 单句超过 15 秒强制切断 |
| 流式连接空闲时被服务端断开 | 静音时主动断开，说话时再重连，用预缓冲补上重连期间的音频 |
| 会议模式外放导致重复识别 | 系统回声消除（M3，会压低其他软件音量约 8dB，可关）+ 文字相似度去重 |
| 长时间开着持续扣费 | VAD 控制：只在有人说话时上传；状态栏显示本次会话时长 |

---

## 9. 开始前需要准备

1. **注册阿里云百炼**，开通"实时语音识别"和"通义千问"，拿到 API Key。新用户一般有免费额度，以官网为准。一个 Key 就能同时用于识别和翻译。
2. （可选）**注册硅基流动**，拿到 Key，用来测试 OpenAI 兼容的分段识别。
3. **自签名证书**：到 M0 打包验证那一步，我会提供创建步骤。

M0 的前三项（采集、断句）不需要 Key，可以先开始做。
