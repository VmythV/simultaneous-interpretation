//! 测试共用：本地模拟的百炼实时识别 WebSocket 服务。
//! 行为：run-task 后延迟 80ms 回 task-started；每 3 帧音频回一次临时结果；
//! finish-task 时回最终结果"收到N字节"和 task-finished；模型名为 fail-model 时回 task-failed 并断开。
#![allow(dead_code, clippy::result_large_err)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tingyi_core::stt::{SttConfig, SttProviderKind};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

pub struct Mock {
    pub url: String,
    pub connections: Arc<AtomicUsize>,
    pub max_frame: Arc<AtomicUsize>,
}

fn event(task_id: &str, name: &str, payload: Value) -> Message {
    Message::Text(json!({"header": {"task_id": task_id, "event": name, "attributes": {}}, "payload": payload}).to_string().into())
}

fn sentence(task_id: &str, text: &str, end: bool) -> Message {
    event(
        task_id,
        "result-generated",
        json!({"output": {"sentence": {"sentence_id": 1, "begin_time": 0, "end_time": null, "text": text, "sentence_end": end, "heartbeat": false, "words": []}}}),
    )
}

pub async fn start_mock() -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/api-ws/v1/inference", listener.local_addr().unwrap());
    let connections = Arc::new(AtomicUsize::new(0));
    let max_frame = Arc::new(AtomicUsize::new(0));
    let (conns, maxf) = (connections.clone(), max_frame.clone());
    tokio::spawn(async move {
        loop {
            let (tcp, _) = listener.accept().await.unwrap();
            conns.fetch_add(1, Ordering::SeqCst);
            let maxf = maxf.clone();
            tokio::spawn(async move {
                let check_auth = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
                    let ok = req.headers().get("Authorization").and_then(|v| v.to_str().ok()) == Some("Bearer test-key");
                    if ok {
                        Ok(resp)
                    } else {
                        let mut err = ErrorResponse::new(Some("bad key".into()));
                        *err.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::UNAUTHORIZED;
                        Err(err)
                    }
                };
                let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, check_auth).await else { return };
                let mut task_id = String::new();
                let mut bytes = 0usize;
                let mut frames = 0usize;
                while let Some(Ok(msg)) = ws.next().await {
                    match msg {
                        Message::Text(t) => {
                            let v: Value = serde_json::from_str(t.as_str()).unwrap();
                            let h = &v["header"];
                            match h["action"].as_str() {
                                Some("run-task") => {
                                    task_id = h["task_id"].as_str().unwrap().to_string();
                                    assert_eq!(h["streaming"], "duplex");
                                    assert_eq!(v["payload"]["parameters"]["format"], "pcm");
                                    assert_eq!(v["payload"]["parameters"]["sample_rate"], 16000);
                                    (bytes, frames) = (0, 0);
                                    if v["payload"]["model"] == "fail-model" {
                                        let mut m = json!({"header": {"task_id": task_id, "event": "task-failed", "error_code": "InvalidParameter", "error_message": "model not enabled", "attributes": {}}, "payload": {}});
                                        m["payload"] = json!({});
                                        ws.send(Message::Text(m.to_string().into())).await.unwrap();
                                        let _ = ws.close(None).await;
                                        return;
                                    }
                                    // 故意晚一点回 task-started，客户端应先缓存音频
                                    tokio::time::sleep(Duration::from_millis(80)).await;
                                    ws.send(event(&task_id, "task-started", json!({}))).await.unwrap();
                                }
                                Some("finish-task") => {
                                    assert_eq!(h["task_id"].as_str().unwrap(), task_id);
                                    ws.send(sentence(&task_id, &format!("收到{bytes}字节"), true)).await.unwrap();
                                    ws.send(event(&task_id, "task-finished", json!({"output": {}, "usage": null}))).await.unwrap();
                                }
                                other => panic!("unexpected action {other:?}"),
                            }
                        }
                        Message::Binary(b) => {
                            assert!(!task_id.is_empty(), "audio before run-task");
                            maxf.fetch_max(b.len(), Ordering::SeqCst);
                            bytes += b.len();
                            frames += 1;
                            if frames.is_multiple_of(3) {
                                ws.send(sentence(&task_id, &format!("部分{frames}"), false)).await.unwrap();
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
    });
    Mock { url, connections, max_frame }
}

pub fn cfg(url: &str, model: &str, key: &str) -> SttConfig {
    SttConfig {
        provider: SttProviderKind::AliyunRealtime,
        base_url: url.to_string(),
        api_key: key.to_string(),
        model: model.to_string(),
        language: None,
        extra_params: None,
    }
}
