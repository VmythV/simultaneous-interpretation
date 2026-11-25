//! 听译核心库：音频采集 → VAD 断句 → 语音识别 → 翻译。
//! 不依赖界面，命令行 demo（M0）和桌面应用（M1 起）共用。

pub mod audio;
pub mod pipeline;
pub mod stt;
pub mod translate;
pub mod vad;
pub mod wav;
