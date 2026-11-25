//! 流水线端到端测试（流式识别）：真实语音 → VAD 断句 → 模拟百炼服务 → 事件。

mod common;

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use common::{cfg, start_mock};
use tingyi_core::audio::{CaptureMsg, Channel};
use tingyi_core::pipeline::{Pipeline, PipelineConfig, PipelineEvent};
use tingyi_core::vad::VadConfig;
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread")]
async fn streaming_pipeline_end_to_end() {
    let mock = start_mock().await;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two_sentences.wav");
    let samples = tingyi_core::wav::read(&fixture).unwrap();

    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel();
    let stt = cfg(&mock.url, "qwen-audio-3.1-asr-flash-streaming", "test-key");
    let pipeline = Pipeline::start(PipelineConfig { vad: VadConfig::default(), stt, translate: None, speculative: true }, ev_tx).unwrap();
    let tx = pipeline.sender();
    let silence = vec![0.0f32; 16_000];
    for chunk in samples.chunks(1600).chain(silence.chunks(1600)) {
        tx.send(CaptureMsg::Audio { channel: Channel::System, samples: chunk.to_vec() }).unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    drop(tx);
    pipeline.shutdown().await;

    let mut events = Vec::new();
    while let Ok(e) = ev_rx.try_recv() {
        events.push(e);
    }

    let errors: Vec<_> = events.iter().filter(|e| matches!(e, PipelineEvent::Error { .. })).collect();
    assert!(errors.is_empty(), "{errors:?}");

    let starts = events.iter().filter(|e| matches!(e, PipelineEvent::SpeechStart { .. })).count();
    let ends = events.iter().filter(|e| matches!(e, PipelineEvent::SpeechEnd { .. })).count();
    assert_eq!((starts, ends), (2, 2), "应断成两句");

    let finals: Vec<(u64, &str)> = events
        .iter()
        .filter_map(|e| match e {
            PipelineEvent::SttFinal { utt_id, text, channel, .. } => {
                assert_eq!(*channel, Channel::System);
                Some((*utt_id, text.as_str()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(finals.len(), 2, "{events:#?}");
    for (_, text) in &finals {
        let bytes: usize = text.trim_start_matches("收到").trim_end_matches("字节").parse().unwrap();
        assert!(bytes > 30_000, "每句应发送 1 秒以上的音频：{text}");
    }
    let ids: HashSet<u64> = finals.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids.len(), 2, "两句的 uttId 应不同");

    // 临时结果和最终结果属于同一个 uttId
    for e in &events {
        if let PipelineEvent::SttPartial { utt_id, .. } = e {
            assert!(ids.contains(utt_id), "临时结果 #{utt_id} 没有对应的最终结果");
        }
    }
}
