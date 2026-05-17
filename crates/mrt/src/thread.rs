pub fn yield_now() {
    unsafe {
        core::arch::asm!(
            "mov x8, #0x01",
            "svc #0",
            out("x8") _,
            options(nostack)
        );
    }
}

pub fn yield_sleep(ticks: u64) {
    let start = current_tick().unwrap_or(0);
    loop {
        if let Some(now) = current_tick() {
            if now.saturating_sub(start) >= ticks {
                break;
            }
        }
        yield_now();
    }
}

fn current_tick() -> Option<u64> {
    let mut buf = [0u8; 128];
    let file = crate::fs::MayaFile::open(b"/proc/sched", false)?;
    let n = file.read(&mut buf);
    if n <= 0 {
        return None;
    }
    parse_tick(&buf[..n as usize])
}

fn parse_tick(buf: &[u8]) -> Option<u64> {
    let key = b"tick:";
    let start = find_subslice(buf, key)? + key.len();
    let mut value = 0u64;
    let mut found = false;
    for &b in &buf[start..] {
        if b.is_ascii_digit() {
            value = value.saturating_mul(10).saturating_add((b - b'0') as u64);
            found = true;
        } else {
            break;
        }
    }
    if found { Some(value) } else { None }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    for idx in 0..=haystack.len() - needle.len() {
        if &haystack[idx..idx + needle.len()] == needle {
            return Some(idx);
        }
    }
    None
}
