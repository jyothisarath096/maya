#!/usr/bin/env python3
"""
Maya OS Intent Canvas Visualizer
Reads UART telemetry from stdin and renders a HUD with pygame.
"""

import json
import math
import os
import sys
import threading

try:
    import pygame
except Exception as exc:  # pragma: no cover
    print(f"[viz] pygame import failed: {exc}", file=sys.stderr)
    raise


state_lock = threading.Lock()
latest_state = None
SCREENSHOT_PATH = os.environ.get("MAYA_VIZ_SHOT", "/private/tmp/maya-viz-latest.png")


def feed_debug(debug_buffer: bytearray, data: bytes) -> None:
    debug_buffer.extend(data)
    while True:
        nl = debug_buffer.find(b"\n")
        if nl < 0:
            break
        line = debug_buffer[:nl].rstrip(b"\r")
        del debug_buffer[: nl + 1]
        if line:
            print(line.decode("utf-8", "replace"), file=sys.stderr)


def reader_thread() -> None:
    global latest_state
    raw = sys.stdin.buffer
    buf = bytearray()
    debug_buf = bytearray()
    marker = b"\x02MAYA"
    while True:
        chunk = raw.read1(4096)
        if not chunk:
            break
        buf.extend(chunk)
        while True:
            start = buf.find(marker)
            if start < 0:
                if buf:
                    feed_debug(debug_buf, bytes(buf))
                    buf.clear()
                break
            if start > 0:
                feed_debug(debug_buf, bytes(buf[:start]))
                del buf[:start]
            end = buf.find(b"\x03", len(marker))
            if end < 0:
                break
            payload = bytes(buf[len(marker):end])
            del buf[: end + 1]
            if buf[:1] == b"\n":
                del buf[:1]
            try:
                data = json.loads(payload.decode("utf-8", "replace"))
            except Exception as exc:
                print(f"[viz] parse error: {exc}", file=sys.stderr)
                continue
            with state_lock:
                latest_state = data
    if debug_buf:
        print(debug_buf.decode("utf-8", "replace"), file=sys.stderr)


threading.Thread(target=reader_thread, daemon=True).start()

pygame.init()
pygame.font.init()

W, H = 1280, 800
screen = pygame.display.set_mode((W, H))
pygame.display.set_caption("Maya OS — Intent Canvas")
clock = pygame.time.Clock()

MONO_SM = pygame.font.SysFont("Courier New", 11)
MONO = pygame.font.SysFont("Courier New", 13)
MONO_BOLD = pygame.font.SysFont("Courier New", 13, bold=True)
MONO_LG = pygame.font.SysFont("Courier New", 26, bold=True)
MONO_XL = pygame.font.SysFont("Courier New", 32, bold=True)

BG = (7, 7, 7)
PANEL = (11, 11, 11)
WHITE = (220, 220, 220)
DIM = (42, 42, 42)
MID = (75, 75, 75)
RED = (200, 34, 0)
RED2 = (255, 51, 0)
RED_DIM = (80, 10, 0)
INACTIVE_TEXT = (140, 140, 140)
STAT_LABEL = (80, 80, 80)
STAT_VALUE = (160, 160, 160)

LW, RW = 252, 228
HEADER_H, FOOTER_H, HEATMAP_H = 33, 28, 80

frame_count = 0
screenshot_saved = False


def intent_name(intent_class: int) -> str:
    return {3: "RT", 1: "CPU", 2: "IO", 4: "BG"}.get(intent_class, "??")


def process_display_name(raw: str) -> str:
    table = {
        "mrt_producer": "MRT_PRODUCER",
        "mrt_prod": "MRT_PRODUCER",
        "mrt_consumer": "MRT_CONSUMER",
        "mrt_cons": "MRT_CONSUMER",
        "mrt_logger": "MRT_LOGGER",
        "mrt_logg": "MRT_LOGGER",
        "mrt_hello": "MRT_HELLO",
        "mrt_hell": "MRT_HELLO",
        "compute_workload": "COMPUTE",
        "compute": "COMPUTE",
        "io_workload": "IO_WORKLOAD",
        "io": "IO_WORKLOAD",
        "background": "BG_TASK",
        "bg": "BG_TASK",
        "background_task": "BG_TASK",
        "matrix_multiply": "MATRIX",
        "matrix": "MATRIX",
        "net_parser": "NET_PARSE",
        "net_pars": "NET_PARSE",
        "sort_suite": "SORT",
        "sort_sui": "SORT",
    }
    if raw in table:
        return table[raw]
    upper = raw.upper().replace("-", "_")
    if len(upper) > 14:
        return upper[:14]
    return upper


def metric_pairs(proc):
    metrics = []
    for key, short in (
        ("ip", "SND"),
        ("ir", "RCV"),
        ("al", "ALM"),
        ("ac", "ACK"),
        ("fw", "WFS"),
        ("pkt", "PKT"),
    ):
        value = proc.get(key, 0)
        if value:
            metrics.append((short, value))
    if not metrics:
        metrics.append(("FRE", proc.get("f", 0)))
    return metrics[:2]


def draw_text(surf, text, x, y, col=WHITE, font=None, anchor="left"):
    font = font or MONO_SM
    image = font.render(str(text), True, col)
    if anchor == "right":
        x -= image.get_width()
    elif anchor == "center":
        x -= image.get_width() // 2
    surf.blit(image, (x, y))
    return image.get_width()


def seg_bar(surf, x, y, w, h, val, maxv, col, bg=(15, 15, 15)):
    pygame.draw.rect(surf, bg, (x, y, w, h))
    if maxv <= 0:
        return
    filled = int(w * min(val, maxv) / maxv)
    seg, gap = 5, 1
    sx = x
    while sx < x + filled:
        sw = min(seg, x + filled - sx)
        pct = (sx - x) / max(filled, 1)
        r = int(col[0] * max(0.3, pct))
        g = int(col[1] * max(0.3, pct))
        b = int(col[2] * max(0.3, pct))
        pygame.draw.rect(surf, (r, g, b), (sx, y, sw, h))
        sx += seg + gap


def draw_node(surf, x, y, size, active, pulse=0.0):
    half = size // 2
    if active:
        glow_r = int(8 + 4 * math.sin(pulse))
        for gr in range(glow_r, 0, -2):
            alpha = int(30 * (gr / max(glow_r, 1)))
            glow = pygame.Surface((size + gr * 2, size + gr * 2), pygame.SRCALPHA)
            glow.fill((255, 51, 0, alpha))
            surf.blit(glow, (x - half - gr, y - half - gr))

    outer = RED2 if active else (20, 20, 20)
    border = RED2 if active else (40, 40, 40)
    inner = (160, 20, 0) if active else (10, 10, 10)
    dot = WHITE if active else (50, 50, 50)

    pygame.draw.rect(surf, outer, (x - half, y - half, size, size), border_radius=2)
    pygame.draw.rect(surf, border, (x - half, y - half, size, size), 1, border_radius=2)
    inner_size = size // 2
    inner_half = inner_size // 2
    pygame.draw.rect(
        surf,
        inner,
        (x - inner_half, y - inner_half, inner_size, inner_size),
        border_radius=1,
    )
    pygame.draw.circle(surf, dot, (x, y), 3)
    bx, by = x + half - 6, y - half
    pygame.draw.line(surf, RED2, (bx + 2, by), (bx + 2, by + 4))
    pygame.draw.line(surf, RED2, (bx, by + 2), (bx + 4, by + 2))


def draw_background():
    screen.fill(BG)
    for gx in range(0, W, 24):
        for gy in range(0, H, 24):
            pygame.draw.rect(screen, (17, 17, 17), (gx, gy, 1, 1))


def draw_header(state):
    pygame.draw.rect(screen, PANEL, (0, 0, W, HEADER_H))
    pygame.draw.line(screen, RED, (0, HEADER_H - 1), (W, HEADER_H - 1), 1)
    draw_text(screen, "MAYA", 18, 10, WHITE, MONO_LG)
    draw_text(screen, "_OS", 58, 10, RED2, MONO_LG)

    px = 115
    for pill in ("PAN", "NET", "FS", "PPO"):
        pygame.draw.rect(screen, RED2, (px, 8, 30, 16), 1)
        draw_text(screen, pill, px + 4, 10, RED2, MONO_SM)
        px += 38

    draw_text(screen, "INTENT CANVAS  ·  AI-NATIVE KERNEL", W // 2, 11, DIM, MONO_SM, "center")
    if state:
        draw_text(screen, f"TICK: {state.get('t', 0):,}", W - 20, 11, WHITE, MONO_SM, "right")


def draw_corner_brackets():
    size, margin = 28, 6
    color = DIM
    for x, y, dx, dy in (
        (margin, margin, 1, 1),
        (W - margin, margin, -1, 1),
        (margin, H - margin, 1, -1),
        (W - margin, H - margin, -1, -1),
    ):
        pygame.draw.lines(screen, color, False, [(x + dx * size, y), (x, y), (x, y + dy * size)], 1)


def draw_left_panel(state):
    pygame.draw.rect(screen, PANEL, (0, HEADER_H, LW, H - HEADER_H - FOOTER_H))
    pygame.draw.line(screen, (26, 26, 26), (LW, HEADER_H), (LW, H - FOOTER_H))
    pygame.draw.rect(screen, (14, 14, 14), (0, HEADER_H, LW, 22))
    pygame.draw.line(screen, (26, 26, 26), (0, HEADER_H + 22), (LW, HEADER_H + 22))
    draw_text(screen, "PROCESS", 10, HEADER_H + 6, STAT_LABEL, MONO_SM)
    draw_text(screen, "CORE PID CPU", LW - 88, HEADER_H + 6, DIM, MONO_SM)

    row_h = 56
    row_y = HEADER_H + 22
    for proc in (state or {}).get("p", []):
        is_active = proc.get("ip", 0) > 0 or proc.get("ir", 0) > 0 or proc.get("fw", 0) > 0
        row_bg = (19, 3, 0) if is_active else (11, 11, 11)
        pygame.draw.rect(screen, row_bg, (0, row_y, LW, row_h))
        pygame.draw.rect(screen, RED2 if is_active else (30, 30, 30), (0, row_y, 3, row_h))

        name = process_display_name(proc.get("n", "?"))
        draw_text(screen, name, 10, row_y + 5, WHITE if is_active else INACTIVE_TEXT, MONO_BOLD)
        badge = intent_name(proc.get("c", 0))
        badge_color = RED2 if is_active else DIM
        badge_w = 34
        badge_x = LW - 72
        pygame.draw.rect(screen, badge_color, (badge_x, row_y + 5, badge_w, 16), 1)
        draw_text(screen, badge, badge_x + 6, row_y + 7, badge_color, MONO_SM)
        draw_text(
            screen,
            f"C{proc.get('k', 0)}  {int(proc.get('s', 0)):>2}%",
            LW - 10,
            row_y + 6,
            WHITE if is_active else INACTIVE_TEXT,
            MONO_SM,
            "right",
        )
        cpu = int(proc.get("s", 0))
        seg_bar(screen, 10, row_y + 25, LW - 20, 5, cpu, 100, RED2 if is_active else (52, 52, 52))

        metrics = metric_pairs(proc)
        block_w = (LW - 28) // 2
        for index, (label, value) in enumerate(metrics):
            mx = 10 + index * block_w
            label_color = RED2 if label in ("ALM", "ACK") and is_active else RED if label in ("ALM", "ACK") else STAT_LABEL
            value_color = WHITE if is_active else STAT_VALUE
            draw_text(screen, label, mx, row_y + 36, label_color, MONO_SM)
            draw_text(screen, f"{value:,}", mx + 36, row_y + 34, value_color, MONO, "left")

        pygame.draw.line(screen, (22, 22, 22), (10, row_y + row_h - 1), (LW - 10, row_y + row_h - 1))
        row_y += row_h

    pygame.draw.line(screen, (26, 26, 26), (0, row_y), (LW, row_y))
    proc_count = len((state or {}).get("p", []))
    draw_text(screen, f"{proc_count} PROCS  8 CORES", 10, row_y + 8, STAT_LABEL, MONO_SM)


def draw_right_panel(state):
    right_x = W - RW
    pygame.draw.rect(screen, PANEL, (right_x, HEADER_H, RW, H - HEADER_H - FOOTER_H))
    pygame.draw.line(screen, (26, 26, 26), (right_x, HEADER_H), (right_x, H - FOOTER_H))
    pygame.draw.rect(screen, (14, 14, 14), (right_x, HEADER_H, RW, 22))
    pygame.draw.line(screen, (26, 26, 26), (right_x, HEADER_H + 22), (W, HEADER_H + 22))
    draw_text(screen, "MAYAFS ALLOCATION", right_x + 10, HEADER_H + 6, STAT_LABEL, MONO_SM)

    fy = HEADER_H + 30
    draw_text(screen, "PATH", right_x + 10, fy, STAT_LABEL, MONO_SM)
    draw_text(screen, "VER", W - 24, fy, STAT_LABEL, MONO_SM, "right")
    fy += 10
    pygame.draw.line(screen, (22, 22, 22), (right_x + 10, fy), (W - 10, fy))
    fy += 8

    file_entries = [
        entry
        for entry in (state or {}).get("fs", [])
        if str(entry.get("p", "")).startswith("/data/")
        or str(entry.get("p", "")).startswith("/sys/")
    ]

    for entry in file_entries[:8]:
        is_active = entry.get("a", 0) == 1
        if is_active:
            pygame.draw.rect(screen, (19, 3, 0), (right_x, fy - 5, RW, 26))
            pygame.draw.rect(screen, RED2, (right_x, fy - 5, 3, 26))
        path = str(entry.get("p", ""))
        path_text = path if len(path) <= 18 else path[-18:]
        draw_text(screen, path_text, right_x + 10, fy, WHITE if is_active else INACTIVE_TEXT, MONO)
        ver = int(entry.get("v", 0))
        draw_text(
            screen,
            f"v{ver}" if ver else "--",
            W - 20,
            fy - 2,
            RED2 if is_active else STAT_VALUE,
            MONO,
            "right",
        )
        if ver:
            bw = int((min(ver, 100) / 100) * (RW - 20))
            pygame.draw.rect(screen, (16, 16, 16), (right_x + 10, fy + 14, RW - 20, 3))
            pygame.draw.rect(screen, RED if is_active else (30, 30, 30), (right_x + 10, fy + 14, bw, 3))
        fy += 28

    pygame.draw.line(screen, (26, 26, 26), (right_x, fy), (W, fy))
    fy += 10
    draw_text(screen, "// NETWORK", right_x + 10, fy + 6, STAT_LABEL, MONO_SM)
    fy += 20
    pygame.draw.rect(screen, (14, 14, 14), (right_x + 10, fy, RW - 20, 30), 1, border_radius=1)
    draw_text(screen, "VIRTIO-NET", right_x + 18, fy + 7, INACTIVE_TEXT, MONO_SM)
    draw_text(screen, "UDP:5555", right_x + 18, fy + 18, STAT_LABEL, MONO_SM)
    pkt_total = sum(proc.get("pkt", 0) for proc in (state or {}).get("p", []))
    draw_text(screen, f"PKT:{pkt_total}", W - 18, fy + 7, RED2, MONO, "right")
    for index in range(12):
        color = RED if index < min(12, max(1, pkt_total % 13)) else (26, 26, 26)
        pygame.draw.rect(screen, color, (right_x + 10 + index * 17, fy + 30, 14, 2))
    fy += 44

    pygame.draw.line(screen, (26, 26, 26), (right_x, fy), (W, fy))
    fy += 10
    draw_text(screen, "// PPO WEIGHTS", right_x + 10, fy + 6, STAT_LABEL, MONO_SM)
    fy += 20
    w_sum = int((state or {}).get("w", 376))
    delta = int((state or {}).get("d", w_sum - 376))
    ws = MONO_XL.render(str(abs(w_sum)), True, RED2)
    screen.blit(ws, (right_x + 10, fy))
    draw_text(screen, "OUT_W SUM", right_x + 90, fy + 4, STAT_LABEL, MONO_SM)
    draw_text(screen, "INIT:376", right_x + 90, fy + 16, DIM, MONO_SM)
    draw_text(screen, f"DELTA:{delta}", right_x + 90, fy + 28, RED2, MONO_SM)
    fy += 52

    pygame.draw.rect(screen, RED_DIM, (right_x + 10, fy, RW - 20, 16), 1)
    draw_text(screen, "ONLINE LEARNING ACTIVE", right_x + 16, fy + 4, RED2, MONO_SM)
    fy += 26

    pygame.draw.line(screen, (26, 26, 26), (right_x, fy), (W, fy))
    fy += 10
    draw_text(screen, "// IPC ALARM FEEDBACK", right_x + 10, fy + 6, STAT_LABEL, MONO_SM)
    fy += 20

    snd = sum(proc.get("ip", 0) for proc in (state or {}).get("p", []))
    rcv = sum(proc.get("ir", 0) for proc in (state or {}).get("p", []))
    alm = sum(proc.get("al", 0) for proc in (state or {}).get("p", []))
    ack = sum(proc.get("ac", 0) for proc in (state or {}).get("p", []))
    for index, (label, value) in enumerate((("SND", snd), ("RCV", rcv), ("ALM", alm), ("ACK", ack))):
        cx = right_x + 10 + (index % 2) * (RW // 2 - 5)
        cy = fy + (index // 2) * 36
        pygame.draw.rect(screen, (18, 18, 18), (cx, cy, RW // 2 - 12, 30), 1, border_radius=1)
        draw_text(screen, label, cx + 8, cy + 6, STAT_LABEL, MONO_SM)
        value_color = RED2 if label in ("ALM", "ACK") else WHITE
        screen.blit(MONO_LG.render(f"{value:,}", True, value_color), (cx + 8, cy + 14))


def draw_orbital(state):
    center_x = LW + (W - LW - RW) // 2
    center_y = HEADER_H + (H - HEADER_H - FOOTER_H - HEATMAP_H) // 2
    rings = (22, 95, 160, 235)

    for ring in range(10):
        radius = 30 + ring * 23
        dot_count = max(8, int(2 * math.pi * radius / 10))
        rotation = frame_count * 0.001 * (1 if ring % 2 == 0 else -1)
        for dot in range(dot_count):
            angle = (dot / dot_count) * math.tau + rotation
            x = int(center_x + math.cos(angle) * radius)
            y = int(center_y + math.sin(angle) * radius)
            if 0 <= x < W and 0 <= y < H:
                twinkle = (dot + ring + frame_count // 8) % 7
                color = RED if twinkle == 1 and dot % 5 == 0 else (50, 50, 50) if twinkle == 0 else (16, 16, 16)
                pygame.draw.rect(screen, color, (x, y, 1, 1))

    pygame.draw.circle(screen, RED, (center_x, center_y), rings[0], 1)
    pygame.draw.circle(screen, RED, (center_x, center_y), rings[1], 1)
    pygame.draw.circle(screen, (20, 20, 20), (center_x, center_y), rings[2], 1)
    pygame.draw.circle(screen, (14, 14, 14), (center_x, center_y), rings[3], 1)
    pygame.draw.circle(screen, RED2, (center_x, center_y), 6)
    pygame.draw.circle(screen, (120, 0, 0), (center_x, center_y), 3)
    draw_text(screen, "PPO", center_x, center_y - 28, RED_DIM, MONO_SM, "center")
    draw_text(screen, "CORE", center_x, center_y + 22, RED_DIM, MONO_SM, "center")
    draw_text(screen, "RT", center_x + rings[1] + 4, center_y - 5, RED_DIM, MONO_SM)
    draw_text(screen, "CPU/IO", center_x + rings[2] + 4, center_y - 5, DIM, MONO_SM)
    draw_text(screen, "WORKERS", center_x + rings[3] + 4, center_y - 5, (22, 22, 22), MONO_SM)

    processes = (state or {}).get("p", [])
    rt_procs = [proc for proc in processes if proc.get("c", 0) == 3]
    mid_procs = [proc for proc in processes if proc.get("id", 0) in (13, 4, 5, 6)]
    outer_procs = [proc for proc in processes if proc.get("id", 0) in (7, 8, 9)]
    groups = ((rt_procs, rings[1], 0.0003), (mid_procs, rings[2], -0.0005), (outer_procs, rings[3], 0.0007))

    positions = {}
    for group, radius, speed in groups:
        count = len(group)
        if not count:
            continue
        rotation = frame_count * speed
        for index, proc in enumerate(group):
            angle = (index / count) * math.tau - math.pi / 2 + rotation
            positions[proc["id"]] = (
                int(center_x + math.cos(angle) * radius),
                int(center_y + math.sin(angle) * radius),
            )

    prod = positions.get(11)
    cons = positions.get(12)
    if prod and cons:
        dx = cons[0] - prod[0]
        dy = cons[1] - prod[1]
        dist = max(1, int(math.hypot(dx, dy)))
        for step in range(dist):
            if step % 10 < 4:
                px = prod[0] + dx * step // dist
                py = prod[1] + dy * step // dist
                pygame.draw.rect(screen, RED, (px, py, 1, 1))
        pulse_t = (frame_count % 60) / 60.0
        pulse_x = int(prod[0] + dx * pulse_t)
        pulse_y = int(prod[1] + dy * pulse_t)
        pygame.draw.circle(screen, RED2, (pulse_x, pulse_y), 3)

    for proc in processes:
        pos = positions.get(proc.get("id"))
        if not pos:
            continue
        active = proc.get("ip", 0) > 0 or proc.get("ir", 0) > 0 or proc.get("fw", 0) > 0
        draw_node(screen, pos[0], pos[1], 20, active, frame_count * 0.08)
        draw_text(
            screen,
            process_display_name(proc.get("n", "")),
            pos[0],
            pos[1] + 14,
            WHITE if active else INACTIVE_TEXT,
            MONO_SM,
            "center",
        )
        if proc.get("ip", 0) > 0:
            metric = f"SND {proc['ip']:,}"
        elif proc.get("ir", 0) > 0:
            metric = f"RCV {proc['ir']:,}"
        elif proc.get("fw", 0) > 0:
            metric = f"WFS {proc['fw']}"
        else:
            metric = f"FRE  {proc.get('f', 0)}"
        draw_text(screen, metric, pos[0], pos[1] + 26, RED2 if active else STAT_LABEL, MONO_SM, "center")


def draw_heatmap(state):
    heat_x = LW
    heat_y = H - FOOTER_H - HEATMAP_H
    heat_w = W - LW - RW
    pygame.draw.rect(screen, (9, 9, 9), (heat_x, heat_y, heat_w, HEATMAP_H))
    pygame.draw.line(screen, RED, (heat_x, heat_y), (heat_x + heat_w, heat_y))
    pygame.draw.line(screen, (26, 26, 26), (heat_x, heat_y), (heat_x, heat_y + HEATMAP_H))
    pygame.draw.line(screen, (26, 26, 26), (heat_x + heat_w, heat_y), (heat_x + heat_w, heat_y + HEATMAP_H))
    draw_text(screen, "// PPO REWARD SIGNAL", heat_x + 10, heat_y + 6, STAT_LABEL, MONO_SM)
    draw_text(screen, f"OUT_W: {(state or {}).get('w', 376)}", heat_x + heat_w - 10, heat_y + 6, RED2, MONO, "right")

    rewards = (state or {}).get("r", [0] * 8)
    col_w = heat_w // 8
    max_h = 50
    for index, reward in enumerate(rewards):
        bx = heat_x + index * col_w + 4
        bw = col_w - 8
        by = heat_y + HEATMAP_H - 14
        pygame.draw.rect(screen, (15, 15, 15), (bx, by - max_h, bw, max_h))
        height = max(2, int(reward / 100 * max_h)) if reward > 0 else 2
        segs = height // 5
        for seg in range(segs):
            pct = seg / max(segs, 1)
            brightness = int(85 + pct * 170)
            pygame.draw.rect(screen, (brightness // 2, 0, 0), (bx + 1, by - (seg + 1) * 5, bw - 2, 4))
        if reward > 0 and segs > 0:
            pygame.draw.rect(screen, RED2, (bx + 1, by - height, bw - 2, 4))
            draw_text(screen, str(reward), bx + bw // 2, by - height - 14, RED2, MONO_SM, "center")
        draw_text(screen, f"C{index}", bx + bw // 2, by + 2, (68, 68, 68) if reward > 0 else (26, 26, 26), MONO_SM, "center")
        if index < 7:
            pygame.draw.line(screen, (17, 17, 17), (heat_x + (index + 1) * col_w, heat_y + 18), (heat_x + (index + 1) * col_w, heat_y + HEATMAP_H))


def draw_footer():
    fy = H - FOOTER_H
    pygame.draw.rect(screen, PANEL, (0, fy, W, FOOTER_H))
    pygame.draw.line(screen, (26, 26, 26), (0, fy), (W, fy))
    draw_text(screen, "MAYAFS V1.0  |  VIRTIO-NET  |  PAN ENFORCED  |  ONLINE PPO LEARNING", 20, fy + 8, DIM, MONO_SM)
    draw_text(screen, "MAYA OS 2026", W - 20, fy + 8, DIM, MONO_SM, "right")


def render(state):
    global frame_count, screenshot_saved
    frame_count += 1
    draw_background()
    draw_header(state)
    draw_corner_brackets()
    draw_left_panel(state)
    draw_right_panel(state)
    draw_orbital(state)
    draw_heatmap(state)
    draw_footer()
    pygame.display.flip()
    if state and not screenshot_saved and frame_count > 20:
        pygame.image.save(screen, SCREENSHOT_PATH)
        screenshot_saved = True


running = True
while running:
    for event in pygame.event.get():
        if event.type == pygame.QUIT:
            running = False
        elif event.type == pygame.KEYDOWN and event.key == pygame.K_q:
            running = False
    with state_lock:
        state = latest_state
    render(state)
    clock.tick(60)

pygame.quit()
