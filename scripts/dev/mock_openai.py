#!/usr/bin/env python3
"""本地模拟的 OpenAI 兼容服务，用于在没有 API Key 时联调流水线和界面（不产生费用）。

- POST /v1/audio/transcriptions：按调用顺序返回预设文本（延迟约 300ms）
- POST /v1/chat/completions（stream=true）：首字约 200ms，之后逐字流式返回 "[目标语言] 原文"

用法：python3 scripts/dev/mock_openai.py [端口，默认 8765]
"""

import json
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CANNED = [
    "今天天气很好，我们下午去公园散步吧。",
    "这个方案的核心是降低延迟。中间只停顿了一下。",
    "好的，明天见。",
    "So the key idea of this paper is simple.",
    "We can reduce the latency by half.",
]
calls = {"stt": 0}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        sys.stderr.write("mock: " + fmt % args + "\n")

    def _body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(n) if n else b""

    def _json(self, code, obj):
        data = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        body = self._body()
        if not self.headers.get("Authorization", "").startswith("Bearer "):
            return self._json(401, {"error": {"message": "missing bearer token"}})
        if self.path.endswith("/audio/transcriptions"):
            time.sleep(0.3)
            text = CANNED[calls["stt"] % len(CANNED)]
            calls["stt"] += 1
            return self._json(200, {"text": text})
        if self.path.endswith("/chat/completions"):
            req = json.loads(body or b"{}")
            system = next((m["content"] for m in req.get("messages", []) if m["role"] == "system"), "")
            user = next((m["content"] for m in reversed(req.get("messages", [])) if m["role"] == "user"), "")
            m = re.search(r"翻译成(\S+?)。", system)
            target = m.group(1) if m else "?"
            reply = f"[{target}] {user}"
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream; charset=utf-8")
            self.send_header("Connection", "close")
            self.end_headers()
            time.sleep(0.2)
            for i in range(0, len(reply), 2):
                chunk = {"choices": [{"index": 0, "delta": {"content": reply[i : i + 2]}}]}
                self.wfile.write(f"data: {json.dumps(chunk, ensure_ascii=False)}\n\n".encode("utf-8"))
                self.wfile.flush()
                time.sleep(0.03)
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            self.close_connection = True
            return
        self._json(404, {"error": {"message": "not found"}})


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    print(f"mock OpenAI-compatible server on http://127.0.0.1:{port}/v1", flush=True)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
