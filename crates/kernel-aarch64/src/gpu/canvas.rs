use core::sync::atomic::{AtomicU32, Ordering};

use super::driver::{flush_all, set_pixel_raw, FB_HEIGHT, FB_WIDTH};
use super::font::{draw_str, draw_u64};

pub const BG: (u8, u8, u8) = (7, 7, 7);
pub const PANEL: (u8, u8, u8) = (11, 11, 11);
pub const WHITE: (u8, u8, u8) = (220, 220, 220);
pub const DIM: (u8, u8, u8) = (26, 26, 26);
pub const MIDGRAY: (u8, u8, u8) = (58, 58, 58);
pub const DIMTXT: (u8, u8, u8) = (55, 55, 55);
pub const RED: (u8, u8, u8) = (200, 34, 0);
pub const RED2: (u8, u8, u8) = (255, 51, 0);

pub const LEFT_W: u32 = 252;
pub const RIGHT_W: u32 = 228;
pub const HEADER_H: u32 = 33;
pub const FOOTER_H: u32 = 28;
pub const HEATMAP_H: u32 = 80;

pub const CX: u32 = LEFT_W + (FB_WIDTH - LEFT_W - RIGHT_W) / 2;
pub const CY: u32 = HEADER_H + (FB_HEIGHT - HEADER_H - FOOTER_H - HEATMAP_H) / 2;

pub const R0: u32 = 22;
pub const R1: u32 = 95;
pub const R2: u32 = 160;
pub const R3: u32 = 235;

static FRAME_COUNTER: AtomicU32 = AtomicU32::new(0);

static COS64: [i16; 64] = [
    1000, 995, 980, 956, 924, 882, 831, 773, 707, 634, 556, 471, 383, 290, 195, 98, 0, -98,
    -195, -290, -383, -471, -556, -634, -707, -773, -831, -882, -924, -956, -980, -995, -1000,
    -995, -980, -956, -924, -882, -831, -773, -707, -634, -556, -471, -383, -290, -195, -98, 0,
    98, 195, 290, 383, 471, 556, 634, 707, 773, 831, 882, 924, 956, 980, 995,
];
static SIN64: [i16; 64] = [
    0, 98, 195, 290, 383, 471, 556, 634, 707, 773, 831, 882, 924, 956, 980, 995, 1000, 995,
    980, 956, 924, 882, 831, 773, 707, 634, 556, 471, 383, 290, 195, 98, 0, -98, -195, -290,
    -383, -471, -556, -634, -707, -773, -831, -882, -924, -956, -980, -995, -1000, -995, -980,
    -956, -924, -882, -831, -773, -707, -634, -556, -471, -383, -290, -195, -98,
];

#[derive(Clone, Copy)]
pub struct ProcessInfo {
    pub pid: u16,
    pub valid: bool,
    pub intent_class: u8,
    pub cpu_ticks: u64,
    pub ipc_sends: u64,
    pub ipc_recvs: u64,
    pub file_writes: u64,
    pub intent_fires: u64,
    pub alm_count: u64,
    pub ack_count: u64,
    pub pkt_count: u64,
    pub core_id: u8,
    pub name: [u8; 16],
    pub name_len: usize,
}

impl ProcessInfo {
    pub const fn empty() -> Self {
        Self {
            pid: 0,
            valid: false,
            intent_class: 0,
            cpu_ticks: 0,
            ipc_sends: 0,
            ipc_recvs: 0,
            file_writes: 0,
            intent_fires: 0,
            alm_count: 0,
            ack_count: 0,
            pkt_count: 0,
            core_id: 0,
            name: [0; 16],
            name_len: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct NodeInfo {
    x: u32,
    y: u32,
    pid: u16,
}

#[derive(Clone, Copy)]
struct FileEntry {
    path: &'static [u8],
    ver: u32,
    has_ver: bool,
    intent: &'static [u8],
    active: bool,
}

pub fn px(x: u32, y: u32, col: (u8, u8, u8)) {
    set_pixel_raw(x, y, col.0, col.1, col.2);
}

pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, col: (u8, u8, u8)) {
    for py in y..y + h {
        for px in x..x + w {
            set_pixel_raw(px, py, col.0, col.1, col.2);
        }
    }
}

pub fn draw_rect_outline(x: u32, y: u32, w: u32, h: u32, col: (u8, u8, u8)) {
    for i in x..x + w {
        set_pixel_raw(i, y, col.0, col.1, col.2);
        set_pixel_raw(i, y + h - 1, col.0, col.1, col.2);
    }
    for i in y..y + h {
        set_pixel_raw(x, i, col.0, col.1, col.2);
        set_pixel_raw(x + w - 1, i, col.0, col.1, col.2);
    }
}

pub fn hline(x: u32, y: u32, w: u32, col: (u8, u8, u8)) {
    for i in x..x + w {
        set_pixel_raw(i, y, col.0, col.1, col.2);
    }
}

pub fn vline(x: u32, y: u32, h: u32, col: (u8, u8, u8)) {
    for i in y..y + h {
        set_pixel_raw(x, i, col.0, col.1, col.2);
    }
}

pub fn fill_circle(cx: u32, cy: u32, r: u32, col: (u8, u8, u8)) {
    let r2 = (r * r) as i64;
    let x0 = cx.saturating_sub(r);
    let x1 = (cx + r).min(FB_WIDTH - 1);
    let y0 = cy.saturating_sub(r);
    let y1 = (cy + r).min(FB_HEIGHT - 1);
    for py in y0..=y1 {
        for px in x0..=x1 {
            let dx = px as i64 - cx as i64;
            let dy = py as i64 - cy as i64;
            if dx * dx + dy * dy <= r2 {
                set_pixel_raw(px, py, col.0, col.1, col.2);
            }
        }
    }
}

pub fn seg_bar(x: u32, y: u32, w: u32, h: u32, val: u32, max: u32, col: (u8, u8, u8), dim: (u8, u8, u8)) {
    fill_rect(x, y, w, h, dim);
    if max == 0 {
        return;
    }
    let filled = ((val as u64 * w as u64) / max as u64) as u32;
    let seg_w = 4u32;
    let seg_gap = 1u32;
    let mut sx = x;
    while sx < x + filled {
        let this_w = seg_w.min(x + filled - sx);
        let pct = (sx - x) * 100 / filled.max(1);
        let r = ((col.0 as u32 * pct / 100).max(col.0 as u32 / 3)) as u8;
        let g = ((col.1 as u32 * pct / 100).max(col.1 as u32 / 3)) as u8;
        let b = ((col.2 as u32 * pct / 100).max(col.2 as u32 / 3)) as u8;
        fill_rect(sx, y, this_w, h, (r, g, b));
        sx += seg_w + seg_gap;
    }
}

pub fn draw_line(x0: u32, y0: u32, x1: u32, y1: u32, col: (u8, u8, u8)) {
    let mut x0 = x0 as i32;
    let mut y0 = y0 as i32;
    let x1 = x1 as i32;
    let y1 = y1 as i32;
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    loop {
        if x0 >= 0 && y0 >= 0 && x0 < FB_WIDTH as i32 && y0 < FB_HEIGHT as i32 {
            set_pixel_raw(x0 as u32, y0 as u32, col.0, col.1, col.2);
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x0 += sx;
        }
        if e2 < dx {
            err += dx;
            y0 += sy;
        }
    }
}

pub fn draw_dashed_line(
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    col: (u8, u8, u8),
    dash: u32,
    gap: u32,
) {
    let dx = x1 as i64 - x0 as i64;
    let dy = y1 as i64 - y0 as i64;
    let len = dx.unsigned_abs().max(dy.unsigned_abs()) as u32;
    if len == 0 {
        return;
    }
    let period = dash + gap;
    for i in 0..len {
        if i % period < dash {
            let px = (x0 as i64 + dx * i as i64 / len as i64) as u32;
            let py = (y0 as i64 + dy * i as i64 / len as i64) as u32;
            if px < FB_WIDTH && py < FB_HEIGHT {
                set_pixel_raw(px, py, col.0, col.1, col.2);
            }
        }
    }
}

pub fn draw_node(cx: u32, cy: u32, sz: u32, active: bool) {
    let outer_col = if active { RED2 } else { (20, 20, 20) };
    let inner_col = if active { RED } else { (10, 10, 10) };
    let border_col = if active { RED2 } else { (40, 40, 40) };
    let dot_col = if active { WHITE } else { (50, 50, 50) };
    let half = sz / 2;

    fill_rect(cx - half, cy - half, sz, sz, outer_col);
    draw_rect_outline(cx - half, cy - half, sz, sz, border_col);

    let inner = sz / 2;
    let ihalf = inner / 2;
    fill_rect(cx - ihalf, cy - ihalf, inner, inner, inner_col);
    fill_circle(cx, cy, 2, dot_col);

    let bx = cx + half - 4;
    let by = cy - half;
    px(bx + 1, by, RED2);
    px(bx, by + 1, RED2);
    px(bx + 1, by + 1, RED2);
    px(bx + 2, by + 1, RED2);
    px(bx + 1, by + 2, RED2);
}

fn draw_circle_outline(cx: u32, cy: u32, r: u32, col: (u8, u8, u8)) {
    for i in 0..128u32 {
        let idx = (i * 64 / 128) as usize % 64;
        let x = cx as i32 + COS64[idx] as i32 * r as i32 / 1000;
        let y = cy as i32 + SIN64[idx] as i32 * r as i32 / 1000;
        if x >= 0 && y >= 0 && x < FB_WIDTH as i32 && y < FB_HEIGHT as i32 {
            set_pixel_raw(x as u32, y as u32, col.0, col.1, col.2);
        }
    }
}

fn pid_to_core(pid: u16) -> u8 {
    match pid {
        11 | 12 => 0,
        10 | 13 => 1,
        4 => 2,
        5 => 3,
        6 => 4,
        7 => 5,
        8 => 6,
        9 => 7,
        _ => 0,
    }
}

pub fn read_all_processes(out: &mut [ProcessInfo; 12]) -> usize {
    for slot in out.iter_mut() {
        *slot = ProcessInfo::empty();
    }
    let mut count = 0usize;
    crate::proc::snapshot_process_display(|pid, name, intent_class, stats| {
        if count >= out.len() {
            return;
        }
        let p = &mut out[count];
        p.pid = pid;
        p.valid = true;
        p.intent_class = intent_class as u8;
        p.cpu_ticks = stats.cpu_ticks_used;
        p.ipc_sends = stats.ipc_sends;
        p.ipc_recvs = stats.ipc_recvs;
        p.file_writes = stats.file_writes;
        p.intent_fires = stats.intent_fire_count;
        p.alm_count = stats.alarms_sent;
        p.ack_count = stats.alarms_acked;
        p.pkt_count = if pid == 8 { stats.intent_fire_count } else { 0 };
        p.core_id = pid_to_core(pid);
        let nlen = name.len().min(16);
        p.name[..nlen].copy_from_slice(&name[..nlen]);
        p.name_len = nlen;
        count += 1;
    });
    count
}

pub fn read_ppo_weights() -> i32 {
    let model = crate::model::weights::load();
    model.out_w.iter().map(|&w| w as i32).sum()
}

pub fn read_core_rewards() -> [i32; 8] {
    crate::sched::policy::get_last_rewards()
}

pub fn draw_background() {
    fill_rect(0, 0, FB_WIDTH, FB_HEIGHT, BG);
    let mut y = 0u32;
    while y < FB_HEIGHT {
        let mut x = 0u32;
        while x < FB_WIDTH {
            set_pixel_raw(x, y, 17, 17, 17);
            x += 24;
        }
        y += 24;
    }
}

pub fn draw_header(tick: u64) {
    fill_rect(0, 0, FB_WIDTH, HEADER_H, PANEL);
    hline(0, HEADER_H, FB_WIDTH, DIM);
    hline(0, HEADER_H - 1, FB_WIDTH, RED);

    draw_str(18, 11, b"MAYA", WHITE, Some(PANEL));
    draw_str(50, 11, b"_OS", RED2, Some(PANEL));

    let mut px0 = 110u32;
    for pill in [b"PAN".as_slice(), b"NET".as_slice(), b"FS ".as_slice(), b"PPO".as_slice()] {
        draw_rect_outline(px0, 8, 28, 14, RED);
        draw_str(px0 + 4, 7, pill, RED2, Some(PANEL));
        px0 += 36;
    }

    draw_str(
        FB_WIDTH / 2 - 120,
        11,
        b"INTENT CANVAS  AI-NATIVE KERNEL",
        DIMTXT,
        Some(PANEL),
    );
    draw_str(FB_WIDTH - 120, 11, b"TICK:", DIMTXT, Some(PANEL));
    draw_u64(FB_WIDTH - 80, 11, tick, WHITE, Some(PANEL));
}

pub fn draw_footer() {
    let fy = FB_HEIGHT - FOOTER_H;
    fill_rect(0, fy, FB_WIDTH, FOOTER_H, PANEL);
    hline(0, fy, FB_WIDTH, DIM);
    draw_str(
        20,
        fy + 11,
        b"MAYAFS V1.0  VIRTIO-NET  PAN ENFORCED  ONLINE PPO LEARNING",
        DIMTXT,
        Some(PANEL),
    );
    draw_str(FB_WIDTH - 130, fy + 11, b"MAYA OS 2026", DIMTXT, Some(PANEL));
}

pub fn draw_left_panel(procs: &[ProcessInfo; 12], count: usize) {
    let px0 = 0u32;
    let py0 = HEADER_H;
    let ph = FB_HEIGHT - HEADER_H - FOOTER_H;
    fill_rect(px0, py0, LEFT_W, ph, PANEL);
    vline(LEFT_W, py0, ph, DIM);

    fill_rect(px0, py0, LEFT_W, 22, (14, 14, 14));
    hline(px0, py0 + 22, LEFT_W, DIM);
    draw_str(px0 + 10, py0 + 14, b"PROCESS", DIMTXT, None);
    draw_str(LEFT_W - 92, py0 + 14, b"CORE PID CPU", MIDGRAY, None);

    let row_h = 46u32;
    let mut ry = py0 + 22;

    for p in procs.iter().take(count.min(12)) {
        if !p.valid {
            continue;
        }
        let is_active = p.intent_class >= 3 && (p.ipc_sends + p.ipc_recvs > 0);
        let row_bg = if is_active {
            (19, 3, 0)
        } else if ((ry - py0) / row_h).is_multiple_of(2) {
            (11, 11, 11)
        } else {
            (9, 9, 9)
        };
        fill_rect(px0, ry, LEFT_W, row_h, row_bg);

        let accent = if is_active {
            RED2
        } else if p.intent_class == 3 {
            (42, 0, 0)
        } else if p.intent_class == 1 {
            (0, 18, 34)
        } else if p.intent_class == 2 {
            (0, 26, 10)
        } else {
            (10, 10, 20)
        };
        fill_rect(px0, ry, 3, row_h, accent);

        let name = &p.name[..p.name_len];
        draw_str(px0 + 10, ry + 13, name, if is_active { WHITE } else { (85, 85, 85) }, None);

        let badge: &[u8] = match p.intent_class {
            3 => b"RT ",
            1 => b"CPU",
            2 => b"IO ",
            4 => b"BG ",
            _ => b"???",
        };
        draw_rect_outline(LEFT_W - 38, ry + 4, 28, 13, if is_active { RED2 } else { (36, 36, 36) });
        draw_str(LEFT_W - 35, ry + 5, badge, if is_active { RED2 } else { DIMTXT }, None);

        draw_str(LEFT_W - 72, ry + 13, b"C", MIDGRAY, None);
        draw_u64(LEFT_W - 64, ry + 13, p.core_id as u64, MIDGRAY, None);
        seg_bar(
            px0 + 10,
            ry + 20,
            LEFT_W - 20,
            3,
            (p.cpu_ticks % 100) as u32,
            100,
            if is_active { RED2 } else { (36, 36, 36) },
            (16, 16, 16),
        );

        let mut mx = px0 + 10u32;
        if p.ipc_sends > 0 {
            draw_str(mx, ry + 34, b"SND", MIDGRAY, None);
            draw_u64(mx + 24, ry + 34, p.ipc_sends, if is_active { (85, 85, 85) } else { MIDGRAY }, None);
            mx += 70;
        }
        if p.ipc_recvs > 0 {
            draw_str(mx, ry + 34, b"RCV", MIDGRAY, None);
            draw_u64(mx + 24, ry + 34, p.ipc_recvs, if is_active { (85, 85, 85) } else { MIDGRAY }, None);
            mx += 70;
        }
        if p.alm_count > 0 {
            draw_str(mx, ry + 34, b"ALM", RED, None);
            draw_u64(mx + 24, ry + 34, p.alm_count, RED2, None);
            mx += 55;
        }
        if p.ack_count > 0 {
            draw_str(mx, ry + 34, b"ACK", RED, None);
            draw_u64(mx + 24, ry + 34, p.ack_count, RED2, None);
            mx += 55;
        }
        if p.file_writes > 0 && mx < LEFT_W - 40 {
            draw_str(mx, ry + 34, b"WFS", MIDGRAY, None);
            draw_u64(mx + 24, ry + 34, p.file_writes, MIDGRAY, None);
            mx += 55;
        }
        if p.pkt_count > 0 && mx < LEFT_W - 40 {
            draw_str(mx, ry + 34, b"PKT", MIDGRAY, None);
            draw_u64(mx + 24, ry + 34, p.pkt_count, MIDGRAY, None);
            mx += 55;
        }
        if p.intent_fires > 0 && mx < LEFT_W - 40 && p.ipc_sends == 0 && p.ipc_recvs == 0 {
            draw_str(mx, ry + 34, b"FRE", MIDGRAY, None);
            draw_u64(mx + 24, ry + 34, p.intent_fires, MIDGRAY, None);
        }

        hline(px0 + 10, ry + row_h - 1, LEFT_W - 20, (19, 19, 19));
        ry += row_h;
    }

    hline(px0, ry, LEFT_W, DIM);
    draw_str(px0 + 10, ry + 14, b"11 PROCS  8 CORES", MIDGRAY, None);
}

pub fn draw_right_panel(ppo_sum: i32, ppo_delta: i32, snd: u64, rcv: u64, alm: u64, ack: u64) {
    let px0 = FB_WIDTH - RIGHT_W;
    let py0 = HEADER_H;
    let ph = FB_HEIGHT - HEADER_H - FOOTER_H;
    fill_rect(px0, py0, RIGHT_W, ph, PANEL);
    vline(px0, py0, ph, DIM);

    fill_rect(px0, py0, RIGHT_W, 22, (14, 14, 14));
    hline(px0, py0 + 22, RIGHT_W, DIM);
    draw_str(px0 + 10, py0 + 14, b"MAYAFS ALLOCATION", DIMTXT, None);

    let mut fy = py0 + 30u32;
    draw_str(px0 + 10, fy, b"PATH", MIDGRAY, None);
    draw_str(px0 + RIGHT_W - 60, fy, b"VER INT", MIDGRAY, None);
    fy += 8;
    hline(px0 + 10, fy, RIGHT_W - 20, (22, 22, 22));
    fy += 8;

    let files = [
        FileEntry { path: b"/data/sensors", ver: 12, has_ver: true, intent: b"RT", active: true },
        FileEntry { path: b"/data/log", ver: 84, has_ver: true, intent: b"IO", active: false },
        FileEntry { path: b"/data/shared", ver: 4, has_ver: true, intent: b"RT", active: false },
        FileEntry { path: b"/sys/io/log", ver: 12, has_ver: true, intent: b"IO", active: false },
        FileEntry { path: b"/proc/11/stat", ver: 0, has_ver: false, intent: b"RT", active: false },
        FileEntry { path: b"/proc/sched", ver: 0, has_ver: false, intent: b"SY", active: false },
    ];

    for f in files {
        if f.active {
            fill_rect(px0, fy - 6, RIGHT_W, 26, (19, 3, 0));
            fill_rect(px0, fy - 6, 3, 26, RED2);
        }
        draw_str(px0 + 10, fy + 4, f.path, if f.active { WHITE } else { (75, 75, 75) }, None);
        if f.has_ver {
            draw_str(px0 + RIGHT_W - 58, fy + 4, b"v", MIDGRAY, None);
            draw_u64(
                px0 + RIGHT_W - 50,
                fy + 4,
                f.ver as u64,
                if f.active { RED2 } else { MIDGRAY },
                None,
            );
        } else {
            draw_str(px0 + RIGHT_W - 58, fy + 4, b"--", (36, 36, 36), None);
        }
        draw_str(px0 + RIGHT_W - 20, fy + 4, f.intent, if f.active { RED2 } else { MIDGRAY }, None);
        if f.has_ver && f.ver > 0 {
            hline(px0 + 10, fy + 10, RIGHT_W - 20, (16, 16, 16));
            let bw = f.ver.min(100) * (RIGHT_W - 20) / 100;
            hline(px0 + 10, fy + 10, bw, if f.active { RED } else { (30, 30, 30) });
        }
        fy += 26;
    }

    hline(px0, fy, RIGHT_W, DIM);
    fy += 10;
    draw_str(px0 + 10, fy + 10, b"// NETWORK", MIDGRAY, None);
    fy += 22;
    draw_rect_outline(px0 + 10, fy, RIGHT_W - 20, 32, DIM);
    draw_str(px0 + 18, fy + 11, b"VIRTIO-NET", (75, 75, 75), None);
    draw_str(px0 + 18, fy + 23, b"UDP:5555", MIDGRAY, None);
    draw_str(px0 + RIGHT_W - 58, fy + 11, b"PKT:", RED2, None);
    draw_u64(px0 + RIGHT_W - 30, fy + 11, 42, RED2, None);
    for s in 0..12u32 {
        let bx = px0 + 10 + s * 17;
        let bc = if s < 7 { RED } else { DIM };
        fill_rect(bx, fy + 32, 14, 2, bc);
    }
    fy += 46;

    hline(px0, fy, RIGHT_W, DIM);
    fy += 10;
    draw_str(px0 + 10, fy + 10, b"// PPO WEIGHTS", MIDGRAY, None);
    fy += 22;
    draw_u64(px0 + 10, fy + 20, ppo_sum.unsigned_abs() as u64, RED2, None);
    draw_str(px0 + 80, fy + 10, b"OUT_W SUM", MIDGRAY, None);
    draw_str(px0 + 80, fy + 22, b"INIT:376", (36, 36, 36), None);
    draw_str(px0 + 80, fy + 34, b"DELTA:", MIDGRAY, None);
    draw_u64(px0 + 122, fy + 34, ppo_delta.unsigned_abs() as u64, RED2, None);
    fy += 46;

    draw_rect_outline(px0 + 10, fy, RIGHT_W - 20, 16, (80, 0, 0));
    draw_str(px0 + 20, fy + 11, b"ONLINE LEARNING ACTIVE", RED2, None);
    fy += 26;

    hline(px0, fy, RIGHT_W, DIM);
    fy += 10;
    draw_str(px0 + 10, fy + 10, b"// IPC ALARM FEEDBACK", MIDGRAY, None);
    fy += 22;

    let counters = [(b"SND".as_slice(), snd), (b"RCV".as_slice(), rcv), (b"ALM".as_slice(), alm), (b"ACK".as_slice(), ack)];
    for (i, (k, v)) in counters.into_iter().enumerate() {
        let cx2 = px0 + 10 + (i as u32 % 2) * (RIGHT_W / 2 - 5);
        let cy2 = fy + (i as u32 / 2) * 36;
        draw_rect_outline(cx2, cy2, RIGHT_W / 2 - 12, 30, (22, 22, 22));
        draw_str(cx2 + 8, cy2 + 12, k, MIDGRAY, None);
        let vc = if k == b"ALM".as_slice() || k == b"ACK".as_slice() { RED2 } else { WHITE };
        draw_u64(cx2 + 8, cy2 + 26, v, vc, None);
    }
}

pub fn draw_orbital(procs: &[ProcessInfo; 12], count: usize, frame: u32) {
    let cx = CX;
    let cy = CY;

    for ring in 0..10u32 {
        let r = 30 + ring * 23;
        let n_dots = ((6 * r) / 30).max(8);
        let rot_offset = (frame / (4 + ring % 3)) % 64;
        let reverse = ring % 2 == 1;
        for d in 0..n_dots {
            let step = 64 / n_dots.max(1);
            let mut idx = (d * step + rot_offset) % 64;
            if reverse {
                idx = (64 - idx) % 64;
            }
            let x = (cx as i32 + COS64[idx as usize] as i32 * r as i32 / 1000) as u32;
            let y = (cy as i32 + SIN64[idx as usize] as i32 * r as i32 / 1000) as u32;
            if x >= FB_WIDTH || y >= FB_HEIGHT {
                continue;
            }
            let twinkle = (d + ring + frame / 8) % 7;
            if twinkle == 1 && d % 5 == 0 {
                set_pixel_raw(x, y, RED.0, RED.1, RED.2);
            } else if twinkle == 0 {
                set_pixel_raw(x, y, 50, 50, 50);
            } else {
                set_pixel_raw(x, y, 16, 16, 16);
            }
        }
    }

    draw_circle_outline(cx, cy, R1, RED);
    draw_circle_outline(cx, cy, R2, (20, 20, 20));
    draw_circle_outline(cx, cy, R3, (14, 14, 14));
    draw_circle_outline(cx, cy, R0, (80, 0, 0));
    fill_circle(cx, cy, 6, RED2);
    fill_circle(cx, cy, 3, (120, 0, 0));

    draw_str(cx + R1 + 4, cy - 4, b"RT", (80, 0, 0), None);
    draw_str(cx + R2 + 4, cy - 4, b"CPU/IO", MIDGRAY, None);
    draw_str(cx + R3 + 4, cy - 4, b"WORKERS", DIM, None);

    let rings_pids = [[11u16, 12, 10, 0], [13, 4, 5, 6], [7, 8, 9, 0]];
    let ring_radii = [R1, R2, R3];
    let ring_speeds = [3u32, 5, 7];
    let mut nodes = [NodeInfo { x: 0, y: 0, pid: 0 }; 12];
    let mut node_count = 0usize;

    for (ri, pids) in rings_pids.iter().enumerate() {
        let n = pids.iter().filter(|&&pid| pid > 0).count() as u32;
        if n == 0 {
            continue;
        }
        let r = ring_radii[ri];
        let base_rot = (frame / ring_speeds[ri]) % 64;
        let mut pi = 0u32;
        for &pid in pids {
            if pid == 0 {
                continue;
            }
            let step = 64 / n;
            let idx = (pi * step + base_rot) % 64;
            let nx = (cx as i32 + COS64[idx as usize] as i32 * r as i32 / 1000) as u32;
            let ny = (cy as i32 + SIN64[idx as usize] as i32 * r as i32 / 1000) as u32;
            if node_count < nodes.len() {
                nodes[node_count] = NodeInfo { x: nx, y: ny, pid };
                node_count += 1;
            }
            pi += 1;
        }
    }

    let mut prod_pos = None;
    let mut cons_pos = None;
    for node in nodes.iter().take(node_count) {
        if node.pid == 11 {
            prod_pos = Some(*node);
        }
        if node.pid == 12 {
            cons_pos = Some(*node);
        }
    }
    if let (Some(p), Some(c)) = (prod_pos, cons_pos) {
        draw_dashed_line(p.x, p.y, c.x, c.y, RED, 4, 6);
        let t = frame % 60;
        let px1 = (p.x as i32 + (c.x as i32 - p.x as i32) * t as i32 / 60) as u32;
        let py1 = (p.y as i32 + (c.y as i32 - p.y as i32) * t as i32 / 60) as u32;
        if px1 < FB_WIDTH && py1 < FB_HEIGHT {
            fill_circle(px1, py1, 2, RED2);
        }
    }

    for node in nodes.iter().take(node_count) {
        let mut proc = None;
        for p in procs.iter().take(count) {
            if p.pid == node.pid {
                proc = Some(*p);
                break;
            }
        }
        let active = proc
            .map(|p| p.ipc_sends > 0 || p.ipc_recvs > 0 || p.file_writes > 0)
            .unwrap_or(false);
        draw_node(node.x, node.y, 18, active);
        if let Some(p) = proc {
            let name = &p.name[..p.name_len.min(8)];
            let lx = node.x.saturating_sub(name.len() as u32 * 4);
            draw_str(lx, node.y + 14, name, if active { WHITE } else { DIMTXT }, None);
        }
    }
}

pub fn draw_heatmap(rewards: &[i32; 8], ppo_sum: i32, _ppo_delta: i32) {
    let hx = LEFT_W;
    let hw = FB_WIDTH - LEFT_W - RIGHT_W;
    let hy = FB_HEIGHT - FOOTER_H - HEATMAP_H;
    let hh = HEATMAP_H;

    fill_rect(hx, hy, hw, hh, (9, 9, 9));
    hline(hx, hy, hw, DIM);
    hline(hx, hy, hw, RED);
    vline(hx, hy, hh, DIM);
    vline(hx + hw - 1, hy, hh, DIM);

    draw_str(hx + 10, hy + 13, b"// PPO REWARD SIGNAL", MIDGRAY, None);
    draw_str(hx + hw - 100, hy + 13, b"OUT_W:", MIDGRAY, None);
    draw_u64(hx + hw - 60, hy + 13, ppo_sum.unsigned_abs() as u64, RED2, None);

    let col_w = hw / 8;
    let bar_w = col_w - 8;
    let max_h = 48u32;

    for i in 0..8u32 {
        let r = rewards[i as usize];
        let bx = hx + i * col_w + 4;
        let by = hy + hh - 16;
        fill_rect(bx, by - max_h, bar_w, max_h, (15, 15, 15));
        let h = if r > 0 {
            (r as u32 * max_h / 100).min(max_h)
        } else {
            2
        };
        let seg_h = 5u32;
        let segs = h / seg_h;
        for s in 0..segs {
            let pct = s * 100 / segs.max(1);
            let brightness = (85 + pct * 170 / 100) as u8;
            fill_rect(bx + 1, by - (s + 1) * seg_h, bar_w - 2, seg_h - 1, (brightness / 2, 0, 0));
        }
        if r > 0 && segs > 0 {
            fill_rect(bx + 1, by - h, bar_w - 2, seg_h - 1, RED2);
        }
        if r > 0 {
            draw_u64(bx + 2, by - h - 8, r as u64, RED2, None);
        }
        let lc = if r > 0 { (68, 68, 68) } else { DIM };
        draw_str(bx + bar_w / 2 - 4, by + 12, b"C", lc, None);
        draw_u64(bx + bar_w / 2 + 4, by + 12, i as u64, lc, None);
        if i < 7 {
            vline(hx + (i + 1) * col_w, hy + 22, hh - 22, (17, 17, 17));
        }
    }
}

pub fn draw_corner_brackets() {
    let s = 28u32;
    let m = 6u32;
    let col = MIDGRAY;
    hline(m, m, s, col);
    vline(m, m, s, col);
    let x = FB_WIDTH - m - s;
    hline(x, m, s, col);
    vline(FB_WIDTH - m, m, s, col);
    hline(m, FB_HEIGHT - m, s, col);
    vline(m, FB_HEIGHT - m - s, s, col);
    hline(FB_WIDTH - m - s, FB_HEIGHT - m, s, col);
    vline(FB_WIDTH - m, FB_HEIGHT - m - s, s, col);
}

pub fn draw_static_layout() {
    draw_background();
    draw_header(0);
    draw_footer();
    draw_corner_brackets();
}

pub fn render_frame() {
    let frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut procs = [ProcessInfo::empty(); 12];
    let count = read_all_processes(&mut procs);
    let rewards = read_core_rewards();
    let ppo_sum = read_ppo_weights();
    let ppo_delta = ppo_sum.saturating_sub(376);
    let tick = crate::arch::timer::current_tick();

    let mut snd = 0u64;
    let mut rcv = 0u64;
    let mut alm = 0u64;
    let mut ack = 0u64;
    for p in procs.iter().take(count) {
        snd += p.ipc_sends;
        rcv += p.ipc_recvs;
        alm += p.alm_count;
        ack += p.ack_count;
    }

    draw_background();
    draw_header(tick);
    draw_footer();
    fill_rect(
        LEFT_W,
        HEADER_H,
        FB_WIDTH - LEFT_W - RIGHT_W,
        FB_HEIGHT - HEADER_H - FOOTER_H - HEATMAP_H,
        BG,
    );
    draw_orbital(&procs, count, frame);
    draw_left_panel(&procs, count);
    draw_right_panel(ppo_sum, ppo_delta, snd, rcv, alm, ack);
    draw_heatmap(&rewards, ppo_sum, ppo_delta);
    draw_corner_brackets();
    vline(LEFT_W, HEADER_H, FB_HEIGHT - HEADER_H - FOOTER_H, DIM);
    vline(FB_WIDTH - RIGHT_W, HEADER_H, FB_HEIGHT - HEADER_H - FOOTER_H, DIM);
    flush_all();
}
