#!/usr/bin/env python3

import csv
import math
import os
import struct
from collections import deque
from pathlib import Path

import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim


EPOCHS = 50
BATCH_SIZE = 256
LR = 0.001
CLIP_EPS = 0.2
GAMMA = 0.99
INTENT_WINDOW_NS = 10_000_000
CPU_WINDOW_NS = 1_000_000_000
TELEMETRY_PATH = Path("telemetry.csv")
OUT_X86 = Path("crates/kernel/model_weights.bin")
OUT_AARCH64 = Path("crates/kernel-aarch64/model_weights.bin")
OUT_PLOT = Path("tools/train/training_loss.png")


def intent_class_weight(intent_class: int) -> float:
    return {
        0: 0.0,
        1: 1.0,
        2: 0.7,
        3: 0.95,
        4: 0.1,
        5: 0.6,
    }.get(intent_class, 0.0)


def safe_float(value: str, default: float = 0.0) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def safe_int(value: str, default: int = 0) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def load_rows(path: Path) -> list[dict]:
    if not path.exists():
        raise SystemExit(f"telemetry file not found: {path}")

    rows: list[dict] = []
    with path.open("r", encoding="utf-8", newline="") as handle:
        sample = handle.read(256)
        handle.seek(0)
        has_header = "intent_id" in sample.lower()

        if has_header:
            reader = csv.DictReader(handle)
            for raw in reader:
                row_type = raw.get("type", "T").strip() or "T"
                pid = safe_int(raw.get("pid"))
                intent_id = safe_int(raw.get("intent_id"))
                intent_class = safe_int(raw.get("intent_class"))
                tick_ns = safe_int(raw.get("tick_ns"))
                anomaly_score = safe_int(raw.get("anomaly_score"))
                rows.append(
                    {
                        "type": row_type,
                        "pid": pid,
                        "intent_id": intent_id,
                        "intent_class": intent_class,
                        "tick_ns": tick_ns,
                        "anomaly_score": anomaly_score,
                    }
                )
        else:
            reader = csv.reader(handle)
            for raw in reader:
                if len(raw) < 6:
                    continue
                rows.append(
                    {
                        "type": raw[0].strip(),
                        "pid": safe_int(raw[1]),
                        "intent_id": safe_int(raw[2]),
                        "intent_class": safe_int(raw[3]),
                        "tick_ns": safe_int(raw[4]),
                        "anomaly_score": safe_int(raw[5]),
                    }
                )
    return rows


def clean_rows(rows: list[dict]) -> list[dict]:
    clean: list[dict] = []
    for row in rows:
        if row["type"] != "T":
            continue
        if row["intent_id"] > 1000:
            continue
        if row["tick_ns"] > 1_000_000_000_000:
            continue
        if row["tick_ns"] == 0:
            continue
        clean.append(row)
    clean.sort(key=lambda row: row["tick_ns"])
    return clean


def build_features(rows: list[dict]) -> tuple[list[list[float]], list[float], list[float]]:
    if not rows:
        return [], [], []

    features: list[list[float]] = []
    rewards: list[float] = []
    old_scores: list[float] = []
    recent_ticks: deque[int] = deque()
    prev_tick = rows[0]["tick_ns"]

    for row in rows:
        tick_ns = row["tick_ns"]
        while recent_ticks and tick_ns - recent_ticks[0] > CPU_WINDOW_NS:
            recent_ticks.popleft()
        recent_ticks.append(tick_ns)

        ticks_per_second = len(recent_ticks)
        cpu_usage_pct = min(1.0, ticks_per_second / 1000.0)
        delta = max(0, tick_ns - prev_tick)
        intent_recency = max(0.0, 1.0 - (delta / INTENT_WINDOW_NS))
        prev_tick = tick_ns

        intent_weight = intent_class_weight(row["intent_class"])
        feature = [
            0.5,
            cpu_usage_pct,
            0.0,
            0.0,
            0.0,
            0.0,
            0.1,
            0.0,
            0.5,
            0.0,
            cpu_usage_pct,
            0.0,
            intent_weight,
            0.0,
            intent_recency,
            0.1,
        ]
        features.append(feature)

        anomaly = float(row["anomaly_score"])
        reward_value = (
            0.5 * feature[12]
            + 0.3 * feature[14]
            - 0.5 * feature[13]
            - 2.0 * (anomaly / 100.0)
            + 0.2 * feature[11]
        )
        rewards.append(float(reward_value))
        old_scores.append(0.5)

    return features, rewards, old_scores


def compute_returns(rewards: list[float], gamma: float = GAMMA) -> list[float]:
    returns: list[float] = []
    running = 0.0
    for reward in reversed(rewards):
        running = reward + gamma * running
        returns.insert(0, running)
    return returns


class MayaPPO(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.l1 = nn.Linear(16, 128)
        self.l2 = nn.Linear(128, 64)
        self.out = nn.Linear(64, 1)

    def __call__(self, x):
        x = nn.relu(self.l1(x))
        x = nn.relu(self.l2(x))
        return mx.sigmoid(self.out(x))


def loss_fn(model: MayaPPO, features, advantages):
    scores = model(features).squeeze(-1)
    return -mx.mean(scores * advantages)


def train_model(model: MayaPPO, features: list[list[float]], rewards: list[float], old_scores: list[float]) -> list[float]:
    if not features:
        raise SystemExit("no telemetry rows left after filtering")

    x_all = mx.array(features, dtype=mx.float32)
    returns = mx.array(compute_returns(rewards), dtype=mx.float32)
    baseline = mx.mean(returns)
    advantages = returns - baseline

    optimizer = optim.Adam(learning_rate=LR)
    losses: list[float] = []
    loss_and_grad_fn = nn.value_and_grad(model, loss_fn)

    sample_count = len(features)
    for epoch in range(1, EPOCHS + 1):
        epoch_loss = 0.0
        batch_count = 0
        for start in range(0, sample_count, BATCH_SIZE):
            end = min(start + BATCH_SIZE, sample_count)
            batch_x = x_all[start:end]
            batch_advantages = advantages[start:end]

            loss, grads = loss_and_grad_fn(model, batch_x, batch_advantages)
            optimizer.update(model, grads)
            mx.eval(model.parameters(), optimizer.state)

            epoch_loss += float(loss.item())
            batch_count += 1

        avg_loss = epoch_loss / max(batch_count, 1)
        losses.append(avg_loss)
        if epoch % 10 == 0 or epoch == 1 or epoch == EPOCHS:
            print(f"Epoch {epoch}/{EPOCHS} loss={avg_loss:.3f}")

    return losses


def quantize_weights(values):
    scaled = mx.round(values * 127.0)
    clipped = mx.clip(scaled, -128, 127)
    return clipped.astype(mx.int8)


def flatten_nested(values):
    flat: list[int] = []
    for row in values:
        if isinstance(row, list):
            flat.extend(flatten_nested(row))
        else:
            flat.append(int(row))
    return flat


def write_int8_array(handle, array):
    flat = flatten_nested(array.tolist())
    handle.write(bytes((value + 256) % 256 for value in flat))


def write_i32_array(handle, values):
    for value in values:
        handle.write(struct.pack("<i", int(value)))


def write_weights(model: MayaPPO, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)

    l1_w = quantize_weights(model.l1.weight)
    l2_w = quantize_weights(model.l2.weight)
    out_w = quantize_weights(model.out.weight.reshape((-1,)))

    scale2 = 127.0 * 127.0
    l1_b = mx.round(model.l1.bias * scale2).astype(mx.int32)
    l2_b = mx.round(model.l2.bias * scale2).astype(mx.int32)
    out_b = mx.round(model.out.bias * scale2).astype(mx.int32)

    with path.open("wb") as handle:
        write_int8_array(handle, l1_w)
        write_i32_array(handle, l1_b.tolist())
        write_int8_array(handle, l2_w)
        write_i32_array(handle, l2_b.tolist())
        write_int8_array(handle, out_w)
        write_i32_array(handle, out_b.tolist())

    print(f"Weights written: {path} ({path.stat().st_size} bytes)")


def maybe_write_loss_plot(losses: list[float]) -> None:
    try:
        import matplotlib.pyplot as plt  # type: ignore
    except ImportError:
        return

    OUT_PLOT.parent.mkdir(parents=True, exist_ok=True)
    plt.figure(figsize=(8, 4))
    plt.plot(range(1, len(losses) + 1), losses)
    plt.xlabel("Epoch")
    plt.ylabel("Loss")
    plt.title("Maya PPO Training Loss")
    plt.tight_layout()
    plt.savefig(OUT_PLOT)
    plt.close()


def main() -> None:
    print("Loading telemetry...")
    rows = clean_rows(load_rows(TELEMETRY_PATH))
    print(f"Rows after filtering: {len(rows)}")

    features, rewards, old_scores = build_features(rows)
    model = MayaPPO()
    losses = train_model(model, features, rewards, old_scores)

    print(f"Final loss={losses[-1]:.3f}")
    write_weights(model, OUT_X86)
    write_weights(model, OUT_AARCH64)
    maybe_write_loss_plot(losses)
    print("Weights ready for kernel embedding")


if __name__ == "__main__":
    main()
