#!/usr/bin/env python3
"""
Maya OS Telemetry Bridge
Reads Maya UART from a local TCP serial socket, extracts telemetry frames,
broadcasts them to WebSocket clients, and forwards shell input/output.
"""

import asyncio
import json
import os
import signal
import subprocess
import sys
import urllib.error
import urllib.request
import webbrowser
from abc import ABC, abstractmethod
from datetime import datetime
from pathlib import Path

import websockets


class InferenceBackend(ABC):
    """
    Pluggable AI inference backend for Maya intent queries.
    Swap between Qwen, APEX, or Claude by setting MAYA_AI_BACKEND.
    """

    @abstractmethod
    def query_sync(self, instruction: str, telemetry: dict) -> str:
        raise NotImplementedError

    def build_system_prompt(self) -> str:
        return (
            "You are the AI reasoning layer of Maya OS, an AI-native AArch64 "
            "operating system running on 8 cores with a PPO online learning "
            "scheduler, MAR (Maya Agentic Runtime) observability shims, MayaFS "
            "versioned filesystem, and capability-gated IPC channels.\n\n"
            "Answer concisely in plain text only. No markdown, no bullet "
            "symbols, no headers. Under 6 lines. Always reference actual "
            "numbers from the telemetry when relevant. If a question asks "
            "about counts, balance, filesystem state, or scheduling, cite the "
            "specific counters or versions directly.\n\n"
            "Intent classes: 0=RealTime, 1=Compute, 2=IO, 4=Background.\n"
            "Key processes: mrt_producer(pid 11), mrt_consumer(12), "
            "mrt_hello(10), mrt_logger(13), mrt_shell(14), "
            "compute_workload(4), matrix_multiply(7), io_workload(5), "
            "net_parser(8), background_task(6), sort_suite(9)."
        )

    def build_user_message(self, instruction: str, telemetry: dict) -> str:
        procs = []
        total_alm = 0
        total_ack = 0
        total_fw = 0
        for proc in telemetry.get("p", []):
            total_alm += proc.get("al", 0)
            total_ack += proc.get("ac", 0)
            total_fw += proc.get("fw", 0)
            procs.append(
                f"pid={proc['id']} {proc['n']} core={proc.get('k', 0)} "
                f"intent={proc.get('c', 0)} "
                f"ip={proc.get('ip', 0)} ir={proc.get('ir', 0)} "
                f"al={proc.get('al', 0)} ac={proc.get('ac', 0)} "
                f"fw={proc.get('fw', 0)}"
            )
        fs_rows = [
            f"{entry.get('p', '?')} v={entry.get('v', 0)} active={entry.get('a', 0)}"
            for entry in telemetry.get("fs", [])
        ]
        return (
            f"Kernel tick: {telemetry.get('t', 0)}\n"
            f"PPO weights: {telemetry.get('w', 0)} delta={telemetry.get('d', 0)}\n"
            f"Per-core rewards: {telemetry.get('r', [])}\n"
            f"Totals: alarms={total_alm} acks={total_ack} fs_writes={total_fw}\n"
            f"Files:\n" + "\n".join(fs_rows) + "\n"
            f"Processes:\n" + "\n".join(procs) + "\n\n"
            f"Query: {instruction}"
        )


class QwenBackend(InferenceBackend):
    """
    Qwen2.5-3B-Instruct via mlx-lm (primary) or Ollama (fallback).
    """

    def __init__(self):
        self.mode = None
        self.mlx_model = None
        self.mlx_tokenizer = None
        self._mlx_generate = None
        self._ollama_model = None
        self._init_backend()

    def _init_backend(self):
        try:
            from mlx_lm import generate, load

            model_path = os.path.expanduser(
                os.environ.get("MAYA_QWEN_MODEL", "~/models/qwen2.5-3b-mlx")
            )
            self.mlx_model, self.mlx_tokenizer = load(model_path)
            self._mlx_generate = generate
            self.mode = "mlx"
            print("[apex] Qwen2.5 loaded via mlx-lm", flush=True)
            return
        except Exception as exc:
            print(f"[apex] mlx-lm unavailable ({exc}), trying Ollama...", flush=True)

        try:
            req = urllib.request.Request("http://localhost:11434/api/tags", method="GET")
            with urllib.request.urlopen(req, timeout=2) as resp:
                if resp.status == 200:
                    self.mode = "ollama"
                    self._ollama_model = os.environ.get("MAYA_OLLAMA_MODEL", "qwen2.5:3b")
                    print(
                        f"[apex] Qwen2.5 loaded via Ollama ({self._ollama_model})",
                        flush=True,
                    )
                    return
        except Exception as exc:
            print(f"[apex] Ollama unavailable ({exc})", flush=True)

        self.mode = "offline"
        print("[apex] WARNING: No inference backend available", flush=True)

    def query_sync(self, instruction: str, telemetry: dict) -> str:
        if self.mode == "offline":
            return "APEX offline: no inference backend available"

        system = self.build_system_prompt()
        user = self.build_user_message(instruction, telemetry)
        prompt = (
            f"<|im_start|>system\n{system}<|im_end|>\n"
            f"<|im_start|>user\n{user}<|im_end|>\n"
            f"<|im_start|>assistant\n"
        )

        if self.mode == "mlx":
            return self._query_mlx(prompt)
        if self.mode == "ollama":
            return self._query_ollama(system, user)
        return "APEX offline: no inference backend available"

    def _query_mlx(self, prompt: str) -> str:
        try:
            response = self._mlx_generate(
                self.mlx_model,
                self.mlx_tokenizer,
                prompt=prompt,
                max_tokens=200,
                verbose=False,
            )
            return response.split("<|im_end|>")[0].strip()
        except Exception as exc:
            return f"Qwen inference error: {str(exc)[:80]}"

    def _query_ollama(self, system: str, user: str) -> str:
        payload = json.dumps(
            {
                "model": self._ollama_model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user},
                ],
                "stream": False,
                "options": {"temperature": 0.3, "num_predict": 200},
            }
        ).encode("utf-8")
        req = urllib.request.Request(
            "http://localhost:11434/api/chat",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                body = json.loads(resp.read().decode("utf-8", "replace"))
            return body["message"]["content"].strip()
        except Exception as exc:
            return f"Ollama error: {str(exc)[:80]}"


class ApexBackend(InferenceBackend):
    """
    APEX inference backend. Slot in when APEX MEDIUM is trained.
    """

    def __init__(self):
        self.endpoint = os.environ.get("MAYA_APEX_ENDPOINT", "http://localhost:8080/v1/generate")
        self.model_path = os.environ.get("MAYA_APEX_MODEL", "")
        print(f"[apex] APEX backend configured at {self.endpoint}", flush=True)

    def query_sync(self, instruction: str, telemetry: dict) -> str:
        payload = json.dumps(
            {
                "messages": [
                    {"role": "system", "content": self.build_system_prompt()},
                    {
                        "role": "user",
                        "content": self.build_user_message(instruction, telemetry),
                    },
                ],
                "max_tokens": 200,
                "temperature": 0.3,
                "turbo_quant": True,
                "kv_budget_mb": 512,
            }
        ).encode("utf-8")
        req = urllib.request.Request(
            self.endpoint,
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                body = json.loads(resp.read().decode("utf-8", "replace"))
            return body["choices"][0]["message"]["content"].strip()
        except Exception as exc:
            return f"APEX offline: {str(exc)[:80]}"


class ClaudeBackend(InferenceBackend):
    """
    Claude API backend. Fallback when local inference unavailable.
    """

    def query_sync(self, instruction: str, telemetry: dict) -> str:
        api_key = os.environ.get("ANTHROPIC_API_KEY")
        if not api_key:
            return "Claude offline: ANTHROPIC_API_KEY not set"
        try:
            import anthropic

            client = anthropic.Anthropic(api_key=api_key)
            response = client.messages.create(
                model="claude-sonnet-4-20250514",
                max_tokens=300,
                system=self.build_system_prompt(),
                messages=[
                    {
                        "role": "user",
                        "content": self.build_user_message(instruction, telemetry),
                    }
                ],
            )
            parts = [getattr(block, "text", "") for block in getattr(response, "content", [])]
            answer = "\n".join(part for part in parts if part).strip()
            return answer or "Claude offline: empty response"
        except Exception as exc:
            return f"Claude offline: {str(exc)[:80]}"


def create_backend() -> InferenceBackend:
    backend_name = os.environ.get("MAYA_AI_BACKEND", "qwen").lower()
    backends = {
        "qwen": QwenBackend,
        "apex": ApexBackend,
        "claude": ClaudeBackend,
    }
    cls = backends.get(backend_name)
    if cls is None:
        print(f'[apex] Unknown backend "{backend_name}", defaulting to Qwen', flush=True)
        cls = QwenBackend
    return cls()


WS_HOST = "localhost"
WS_PORT = 8765
SERIAL_HOST = "127.0.0.1"
SERIAL_PORT = 4444
SERIAL_RETRY_S = 0.5

latest_state = None
latest_telemetry = {}
clients = set()
serial_writer = None
serial_lock = None
debug_log = None
bridge_dir = Path(__file__).resolve().parent
training_log = bridge_dir / "maya-training-data.jsonl"
AI_BACKEND = create_backend()


def cleanup_old_bridges():
    current_pid = os.getpid()
    result = subprocess.run(
        ["pgrep", "-f", "maya-bridge.py"],
        capture_output=True,
        text=True,
    )
    if result.returncode not in (0, 1):
        return
    for line in result.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            pid = int(line)
        except ValueError:
            continue
        if pid == current_pid:
            continue
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass


async def broadcast_json(obj):
    if not clients:
        return
    msg = json.dumps(obj)
    dead = set()
    for ws in tuple(clients):
        try:
            await ws.send(msg)
        except Exception:
            dead.add(ws)
    for ws in dead:
        clients.discard(ws)


async def broadcast_telemetry(state):
    global latest_state, latest_telemetry
    latest_state = state
    latest_telemetry = state
    await broadcast_json({"type": "telemetry", "data": state})


async def broadcast_shell_output(text, intent_response=False):
    payload = {"type": "shell_output", "text": text}
    if intent_response:
        payload["intent_response"] = True
    await broadcast_json(payload)


async def send_serial_text(text):
    global serial_writer
    if not text or serial_writer is None:
        return
    async with serial_lock:
        try:
            serial_writer.write(text.encode("utf-8", "replace"))
            await serial_writer.drain()
        except Exception:
            serial_writer = None


def log_training_example(query: str, telemetry: dict, response: str):
    example = {
        "timestamp": datetime.utcnow().isoformat(),
        "backend": os.environ.get("MAYA_AI_BACKEND", "qwen"),
        "instruction": query,
        "context": {
            "tick": telemetry.get("t", 0),
            "ppo_weights": telemetry.get("w", 0),
            "rewards": telemetry.get("r", []),
            "processes": telemetry.get("p", []),
            "files": telemetry.get("fs", []),
        },
        "response": response,
    }
    try:
        with training_log.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(example) + "\n")
    except Exception:
        pass


async def handle_intent_query(query: str, loop: asyncio.AbstractEventLoop):
    await broadcast_shell_output(f"? {query}")

    telemetry_snapshot = dict(latest_telemetry)
    telemetry_snapshot["p"] = [dict(proc) for proc in latest_telemetry.get("p", [])]
    telemetry_snapshot["r"] = list(latest_telemetry.get("r", []))
    telemetry_snapshot["fs"] = [dict(entry) for entry in latest_telemetry.get("fs", [])]
    try:
        answer = await loop.run_in_executor(
            None,
            AI_BACKEND.query_sync,
            query,
            telemetry_snapshot,
        )
    except Exception as exc:
        answer = f"inference error: {str(exc)[:80]}"

    for line in answer.splitlines():
        line = line.strip()
        if line:
            await broadcast_shell_output(line, intent_response=True)
    await broadcast_shell_output("maya>")
    log_training_example(query, telemetry_snapshot, answer)


async def handle_ws_message(raw):
    try:
        msg = json.loads(raw)
    except json.JSONDecodeError:
        return
    if msg.get("type") != "shell_input":
        return

    text = msg.get("text", "")
    stripped = text.strip()
    if stripped.startswith("?"):
        loop = asyncio.get_running_loop()
        await handle_intent_query(stripped[1:].strip(), loop)
        return
    await send_serial_text(text)


async def ws_handler(websocket):
    clients.add(websocket)
    if latest_state is not None:
        await websocket.send(json.dumps({"type": "telemetry", "data": latest_state}))
    try:
        async for raw in websocket:
            await handle_ws_message(raw)
    finally:
        clients.discard(websocket)


async def ws_server():
    async with websockets.serve(
        ws_handler,
        WS_HOST,
        WS_PORT,
        reuse_address=True,
        reuse_port=True,
    ):
        await asyncio.Future()


class SerialParser:
    def __init__(self):
        self.buf = bytearray()
        self.debug = bytearray()
        self.telemetry_marker = b"\x02MAYA"
        self.shell_marker = b"\x01SHELL "

    def log_debug_line(self, raw: bytes):
        if debug_log is None:
            return
        text = raw.replace(b"\r", b"").decode("utf-8", "replace").rstrip("\n")
        if not text:
            return
        timestamp = datetime.now().isoformat(timespec="seconds")
        debug_log.write(f"{timestamp} {text}\n")
        debug_log.flush()

    def flush_debug(self, force=False):
        while True:
            newline = self.debug.find(b"\n")
            if newline >= 0:
                line = bytes(self.debug[: newline + 1])
                del self.debug[: newline + 1]
                self.log_debug_line(line)
                continue
            if force and self.debug:
                line = bytes(self.debug)
                self.debug.clear()
                self.log_debug_line(line)
            break

    async def feed(self, chunk):
        self.buf.extend(chunk)
        while True:
            telem_idx = self.buf.find(self.telemetry_marker)
            shell_idx = self.buf.find(self.shell_marker)
            marker_positions = [idx for idx in (telem_idx, shell_idx) if idx >= 0]

            if marker_positions:
                start = min(marker_positions)
                if start > 0:
                    prefix = bytes(self.buf[:start])
                    del self.buf[:start]
                    self.debug.extend(prefix)
                    self.flush_debug()
                    continue

                if self.buf.startswith(self.telemetry_marker):
                    end = self.buf.find(b"\x03", len(self.telemetry_marker))
                    if end < 0:
                        break
                    payload = bytes(self.buf[len(self.telemetry_marker):end])
                    frame_end = end + 1
                    if len(self.buf) > frame_end and self.buf[frame_end:frame_end + 1] == b"\n":
                        frame_end += 1
                    del self.buf[:frame_end]
                    try:
                        data = self.parse_telemetry_payload(payload)
                        await broadcast_telemetry(data)
                    except Exception as exc:
                        print(f"[bridge] {exc}", file=sys.stderr, flush=True)
                    continue

                if self.buf.startswith(self.shell_marker):
                    end = self.buf.find(b"\n", len(self.shell_marker))
                    if end < 0:
                        break
                    payload = bytes(self.buf[len(self.shell_marker):end]).replace(b"\r", b"")
                    del self.buf[: end + 1]
                    await broadcast_shell_output(payload.decode("utf-8", "replace"))
                    continue

            newline = self.buf.find(b"\n")
            if newline < 0:
                break

            line = bytes(self.buf[:newline])
            if b"\x02" in line or b"\x01" in line:
                break

            self.log_debug_line(line)
            del self.buf[: newline + 1]

    async def finish(self):
        if self.buf and b"\x02" not in self.buf and b"\x01" not in self.buf:
            self.debug.extend(self.buf)
            self.buf.clear()
        self.flush_debug(force=True)

    def parse_telemetry_payload(self, payload: bytes):
        candidates = [payload]
        json_start = payload.find(b"{")
        json_end = payload.rfind(b"}")
        if json_start >= 0 and json_end > json_start:
            candidates.append(payload[json_start : json_end + 1])

        for candidate in candidates:
            try:
                return json.loads(candidate.decode("utf-8", "replace"))
            except Exception:
                cleaned = bytes(b for b in candidate if b in (9, 10, 13) or 32 <= b <= 126)
                try:
                    return json.loads(cleaned.decode("utf-8", "replace"))
                except Exception:
                    continue
        raise ValueError("unable to parse telemetry payload")


async def serial_task():
    global serial_writer
    parser = SerialParser()
    while True:
        try:
            reader, writer = await asyncio.open_connection(SERIAL_HOST, SERIAL_PORT)
            serial_writer = writer
            print(f"[bridge] serial connected {SERIAL_HOST}:{SERIAL_PORT}", flush=True)
            while True:
                chunk = await reader.read(4096)
                if not chunk:
                    break
                await parser.feed(chunk)
            await parser.finish()
        except Exception:
            serial_writer = None
            await asyncio.sleep(SERIAL_RETRY_S)
            continue
        finally:
            if serial_writer is not None:
                try:
                    serial_writer.close()
                    await serial_writer.wait_closed()
                except Exception:
                    pass
            serial_writer = None
        print("[bridge] serial disconnected", flush=True)
        await asyncio.sleep(SERIAL_RETRY_S)


async def main():
    global serial_lock, debug_log
    cleanup_old_bridges()
    await asyncio.sleep(0.3)
    serial_lock = asyncio.Lock()
    debug_log = open(bridge_dir / "maya-debug.log", "w", encoding="utf-8")
    viz_path = bridge_dir / "maya-hud.html"
    webbrowser.open(f"file://{viz_path}")
    await asyncio.gather(ws_server(), serial_task())


if __name__ == "__main__":
    asyncio.run(main())
