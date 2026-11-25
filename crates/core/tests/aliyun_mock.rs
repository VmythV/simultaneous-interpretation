//! 用本地模拟的百炼 WebSocket 服务测试流式识别客户端的协议状态机。

mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use common::{cfg, start_mock};
use tingyi_core::stt::SttOut;
use tingyi_core::stt::aliyun::AliyunStream;
use tokio::sync::mpsc;

/// 发送一句话（samples 个采样，分小块立即发送），收集直到任务结束
async fn utterance(stream: &AliyunStream, rx: &mut mpsc::UnboundedReceiver<SttOut>, samples: usize) -> Vec<SttOut> {
    stream.begin();
    for chunk in vec![0.1f32; samples].chunks(512) {
        stream.audio(chunk.to_vec());
    }
    stream.end();
    let mut outs = Vec::new();
    loop {
        let out = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("timeout").unwrap();
        let done = matches!(out, SttOut::TaskDone { .. } | SttOut::Error(_));
        outs.push(out);
        if done {
            return outs;
        }
    }
}

fn final_text(outs: &[SttOut]) -> Option<String> {
    outs.iter().find_map(|o| match o {
        SttOut::Final { text, .. } => Some(text.clone()),
        _ => None,
    })
}

#[tokio::test]
async fn streams_buffers_and_reuses_connection() {
    let mock = start_mock().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let stream = AliyunStream::spawn(cfg(&mock.url, "qwen-audio-3.1-asr-flash-streaming", "test-key"), tx);

    // 1 秒音频 = 16000 采样 = 32000 字节，全部在 task-started 之前送达
    let outs = utterance(&stream, &mut rx, 16_000).await;
    assert_eq!(final_text(&outs).as_deref(), Some("收到32000字节"), "{outs:?}");
    assert!(outs.iter().any(|o| matches!(o, SttOut::Partial { .. })), "应有临时结果：{outs:?}");
    assert!(mock.max_frame.load(Ordering::SeqCst) <= 3200, "每帧不超过 3200 字节");

    // 第二句：复用同一条连接
    let outs = utterance(&stream, &mut rx, 8_000).await;
    assert_eq!(final_text(&outs).as_deref(), Some("收到16000字节"), "{outs:?}");
    assert_eq!(mock.connections.load(Ordering::SeqCst), 1, "应复用连接");
}

#[tokio::test]
async fn task_failure_reports_error_and_reconnects() {
    let mock = start_mock().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let failing = AliyunStream::spawn(cfg(&mock.url, "fail-model", "test-key"), tx.clone());
    let outs = utterance(&failing, &mut rx, 3_200).await;
    match outs.last() {
        Some(SttOut::Error(e)) => assert!(e.contains("model not enabled"), "{e}"),
        other => panic!("expected error, got {other:?}"),
    }

    // 同一个句柄的下一句会重新连接（task-failed 后旧连接作废）
    let outs = utterance(&failing, &mut rx, 3_200).await;
    assert!(matches!(outs.last(), Some(SttOut::Error(_))));
    assert_eq!(mock.connections.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn bad_key_is_reported() {
    let mock = start_mock().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let stream = AliyunStream::spawn(cfg(&mock.url, "qwen-audio-3.1-asr-flash-streaming", "wrong"), tx);
    let outs = utterance(&stream, &mut rx, 1_600).await;
    match outs.last() {
        Some(SttOut::Error(e)) => assert!(e.contains("401") && e.contains("API Key"), "{e}"),
        other => panic!("expected error, got {other:?}"),
    }
}
