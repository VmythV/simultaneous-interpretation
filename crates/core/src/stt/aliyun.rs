//! 阿里云百炼实时语音识别（DashScope WebSocket，run-task / duplex 协议）
//!
//! 适用模型：qwen-audio-3.1-asr-flash-streaming（官方推荐）、fun-asr-realtime、paraformer-realtime-v2。
//! 协议要点：
//! - 连接 wss://dashscope.aliyuncs.com/api-ws/v1/inference，Header `Authorization: Bearer <key>`
//! - 文本帧发 JSON 指令，二进制帧发音频（16kHz 16-bit 单声道 PCM，每 100ms 发 3200 字节）
//! - run-task → 等 task-started → 发音频 → finish-task → 读到 task-finished
//! - 一条连接可以依次跑多个任务；约 60 秒没有新任务会被服务端关闭；task-failed 后连接不可再用
//!
//! 本实现：VAD 每断出一句话就开一个任务，连接复用，省去静音时的计费。

use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::{SttConfig, SttOut};
use crate::wav::f32_to_i16;

/// 每个音频帧 100ms = 1600 采样 = 3200 字节
const FRAME_SAMPLES: usize = 1600;
/// 连接空闲超过这个时间就主动重连（服务端约 60 秒关闭）
const MAX_IDLE: Duration = Duration::from_secs(45);
const START_TIMEOUT: Duration = Duration::from_secs(5);
const FINISH_TIMEOUT: Duration = Duration::from_secs(8);

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

enum Cmd {
    Begin,
    Audio(Vec<f32>),
    End,
}

/// 流式识别句柄。按 begin → audio… → end 的顺序调用；结果从创建时给的 `out` 通道发出。
pub struct AliyunStream {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl AliyunStream {
    pub fn spawn(cfg: SttConfig, out: mpsc::UnboundedSender<SttOut>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(actor(cfg, rx, out));
        Self { tx }
    }

    /// 一句话开始（会立即建立/复用连接并开始任务）
    pub fn begin(&self) {
        let _ = self.tx.send(Cmd::Begin);
    }

    pub fn audio(&self, samples: Vec<f32>) {
        let _ = self.tx.send(Cmd::Audio(samples));
    }

    /// 一句话结束：发送剩余音频和 finish-task，服务端随后返回最终结果
    pub fn end(&self) {
        let _ = self.tx.send(Cmd::End);
    }
}

struct Conn {
    ws: Ws,
    last_used: Instant,
}

async fn connect(cfg: &SttConfig) -> Result<Conn> {
    let mut req = cfg.base_url.as_str().into_client_request().context("识别服务地址无效")?;
    req.headers_mut().insert("Authorization", format!("Bearer {}", cfg.api_key).parse()?);
    let (ws, _) = tokio_tungstenite::connect_async(req).await.map_err(|e| match e {
        tokio_tungstenite::tungstenite::Error::Http(resp) => {
            let body = resp.body().as_ref().map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
            anyhow!("连接识别服务被拒绝（HTTP {}）：{}，请检查 API Key", resp.status(), body)
        }
        other => anyhow!("连接识别服务失败：{other}"),
    })?;
    Ok(Conn { ws, last_used: Instant::now() })
}

async fn actor(cfg: SttConfig, mut rx: mpsc::UnboundedReceiver<Cmd>, out: mpsc::UnboundedSender<SttOut>) {
    let mut conn: Option<Conn> = None;
    let mut task_seq: u64 = 0;
    while let Some(cmd) = rx.recv().await {
        // 只有 Begin 才开始一个任务；之前残留的音频 / 结束指令直接丢弃
        if !matches!(cmd, Cmd::Begin) {
            continue;
        }
        task_seq += 1;
        if let Err(e) = run_task(&cfg, &mut conn, &mut rx, &out, task_seq).await {
            conn = None;
            let _ = out.send(SttOut::Error(e.to_string()));
        }
    }
    if let Some(mut c) = conn {
        let _ = c.ws.close(None).await;
    }
}

fn header(action: &str, task_id: &str) -> Value {
    json!({ "action": action, "task_id": task_id, "streaming": "duplex" })
}

fn run_task_message(cfg: &SttConfig, task_id: &str) -> String {
    let hints: Vec<String> = match cfg.language.as_deref() {
        None | Some("") | Some("auto") => vec!["zh".into(), "en".into()],
        Some(lang) => vec![lang.to_string()],
    };
    let mut parameters = json!({ "format": "pcm", "sample_rate": 16000, "language_hints": hints });
    if let (Some(Value::Object(extra)), Value::Object(map)) = (&cfg.extra_params, &mut parameters) {
        for (k, v) in extra {
            map.insert(k.clone(), v.clone());
        }
    }
    json!({
        "header": header("run-task", task_id),
        "payload": {
            "task_group": "audio",
            "task": "asr",
            "function": "recognition",
            "model": cfg.model,
            "parameters": parameters,
            "input": {}
        }
    })
    .to_string()
}

async fn send_frames(ws: &mut Ws, pending: &mut Vec<i16>, all: bool) -> Result<()> {
    while pending.len() >= FRAME_SAMPLES || (all && !pending.is_empty()) {
        let n = pending.len().min(FRAME_SAMPLES);
        let bytes: Vec<u8> = pending.drain(..n).flat_map(|s| s.to_le_bytes()).collect();
        ws.send(Message::Binary(bytes.into())).await.context("发送音频失败")?;
    }
    Ok(())
}

async fn run_task(
    cfg: &SttConfig,
    conn: &mut Option<Conn>,
    rx: &mut mpsc::UnboundedReceiver<Cmd>,
    out: &mpsc::UnboundedSender<SttOut>,
    task_seq: u64,
) -> Result<()> {
    if conn.as_ref().is_none_or(|c| c.last_used.elapsed() > MAX_IDLE) {
        if let Some(mut old) = conn.take() {
            let _ = old.ws.close(None).await;
        }
        *conn = Some(connect(cfg).await?);
    }
    let c = conn.as_mut().unwrap();
    let task_id = uuid::Uuid::new_v4().simple().to_string();
    c.ws.send(Message::Text(run_task_message(cfg, &task_id).into())).await.context("发送 run-task 失败")?;

    let mut started = false;
    let mut ending = false;
    let mut pending: Vec<i16> = Vec::new();
    let mut deadline = Instant::now() + START_TIMEOUT;

    loop {
        tokio::select! {
            cmd = rx.recv(), if !ending => {
                match cmd {
                    Some(Cmd::Audio(samples)) => {
                        pending.extend(samples.into_iter().map(f32_to_i16));
                        if started {
                            send_frames(&mut c.ws, &mut pending, false).await?;
                        }
                    }
                    // 新的一句在上一句结束前到来（不应发生），按结束处理
                    Some(Cmd::End) | Some(Cmd::Begin) | None => {
                        ending = true;
                        if started {
                            send_frames(&mut c.ws, &mut pending, true).await?;
                            c.ws.send(Message::Text(json!({"header": header("finish-task", &task_id), "payload": {"input": {}}}).to_string().into())).await?;
                            deadline = Instant::now() + FINISH_TIMEOUT;
                        }
                    }
                }
            }
            msg = c.ws.next() => {
                let text = match msg {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(frame))) => bail!("识别服务关闭了连接：{frame:?}"),
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => bail!("识别连接出错：{e}"),
                    None => bail!("识别连接已断开"),
                };
                let v: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                let h = &v["header"];
                if h["task_id"].as_str().is_some_and(|id| id != task_id) {
                    continue;
                }
                match h["event"].as_str() {
                    Some("task-started") => {
                        started = true;
                        send_frames(&mut c.ws, &mut pending, ending).await?;
                        if ending {
                            c.ws.send(Message::Text(json!({"header": header("finish-task", &task_id), "payload": {"input": {}}}).to_string().into())).await?;
                            deadline = Instant::now() + FINISH_TIMEOUT;
                        } else {
                            deadline = Instant::now() + Duration::from_secs(3600);
                        }
                    }
                    Some("result-generated") => {
                        let s = &v["payload"]["output"]["sentence"];
                        if s["heartbeat"].as_bool() == Some(true) {
                            continue;
                        }
                        let sentence_id = s["sentence_id"].as_u64().unwrap_or(0);
                        let key = task_seq * 10_000 + sentence_id;
                        let text = s["text"].as_str().unwrap_or_default().to_string();
                        if s["sentence_end"].as_bool() == Some(true) {
                            let _ = out.send(SttOut::Final { key, text });
                        } else if !text.is_empty() {
                            let _ = out.send(SttOut::Partial { key, text });
                        }
                    }
                    Some("task-finished") => {
                        c.last_used = Instant::now();
                        let _ = out.send(SttOut::TaskDone { task: task_seq });
                        return Ok(());
                    }
                    Some("task-failed") => {
                        bail!(
                            "识别失败：{}（{}）",
                            h["error_message"].as_str().unwrap_or("?"),
                            h["error_code"].as_str().unwrap_or("?")
                        );
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(deadline.into()) => {
                bail!(if started { "等待识别结果超时" } else { "识别任务启动超时" });
            }
        }
    }
}
