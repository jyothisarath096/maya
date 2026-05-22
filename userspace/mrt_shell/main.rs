#![no_std]
#![no_main]

use mrt::fs::{query_by_intent, MayaFile};
use mrt::input;
use mrt::intent::{self, IntentClass};
use mrt::io::MrtFile;
use mrt::net::MayaNet;
use mrt::thread;

struct ShellOut {
    stdout: MrtFile,
}

impl ShellOut {
    fn new(stdout: MrtFile) -> Self {
        Self { stdout }
    }

    fn line(&mut self, text: &[u8]) {
        let _ = self.stdout.write_shell_frame(text);
    }

    fn prompt(&mut self) {
        self.line(b"maya>");
    }

    fn block(&mut self, text: &[u8]) {
        if text.is_empty() {
            return;
        }
        let mut start = 0usize;
        for (idx, &b) in text.iter().enumerate() {
            if b == b'\n' {
                let mut end = idx;
                if end > start && text[end - 1] == b'\r' {
                    end -= 1;
                }
                self.line(&text[start..end]);
                start = idx + 1;
            }
        }
        if start < text.len() {
            self.line(&text[start..]);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    shell_main();
    loop {
        thread::yield_now();
    }
}

fn shell_main() {
    let _cap = intent::register(b"mrt_shell", IntentClass::IO);
    let stdout = match MrtFile::stdout() {
        Some(file) => file,
        None => loop {
            thread::yield_now();
        },
    };
    let mut shell = ShellOut::new(stdout);

    shell.line(b"");
    shell.line(b"  MAYA OS SHELL v1.0");
    shell.line(b"  AI-NATIVE KERNEL");
    shell.line(b"  type 'help' for commands");
    shell.line(b"");
    shell.prompt();

    let mut line_buf = [0u8; 128];
    let mut line_len = 0usize;

    loop {
        let ch = input::read_char();
        match ch {
            0 => thread::yield_now(),
            b'\n' | b'\r' => {
                execute_command(&line_buf[..line_len], &mut shell);
                line_len = 0;
                shell.prompt();
            }
            b'\x08' | 127 => {
                if line_len > 0 {
                    line_len -= 1;
                }
            }
            32..=126 => {
                if line_len < line_buf.len() - 1 {
                    line_buf[line_len] = ch;
                    line_len += 1;
                }
            }
            _ => {}
        }
    }
}

fn execute_command(cmd: &[u8], shell: &mut ShellOut) {
    let cmd = trim(cmd);
    if cmd.is_empty() {
        return;
    }

    let (verb, args) = split_first_word(cmd);
    if verb == b"help" {
        cmd_help(shell);
    } else if verb == b"ls" {
        cmd_ls(args, shell);
    } else if verb == b"cat" {
        cmd_cat(args, shell);
    } else if verb == b"hist" {
        cmd_history(args, shell);
    } else if verb == b"ps" {
        cmd_ps(shell);
    } else if verb == b"ppo" {
        cmd_ppo(shell);
    } else if verb == b"net" {
        cmd_net(shell);
    } else if verb == b"insights" {
        cmd_insights(shell);
    } else if verb == b"health" {
        cmd_health(shell);
    } else if verb == b"reward" {
        cmd_reward(shell);
    } else if verb == b"versions" {
        cmd_versions(shell);
    } else if verb == b"watch" {
        cmd_watch(args, shell);
    } else if verb == b"clear" {
        cmd_clear(shell);
    } else if verb == b"intent" {
        cmd_intent(args, shell);
    } else if verb == b"tag" {
        cmd_tag(args, shell);
    } else if verb == b"find" {
        shell.line(b"find: tag query syscall not exposed yet");
    } else if verb.starts_with(b"v") && verb.len() > 1 {
        cmd_read_version(&verb[1..], args, shell);
    } else {
        let mut line = [0u8; 160];
        let mut pos = 0usize;
        pos = push_bytes(&mut line, pos, b"unknown: ");
        pos = push_bytes(&mut line, pos, verb);
        shell.line(&line[..pos]);
    }
}

fn cmd_help(shell: &mut ShellOut) {
    shell.line(b"MAYA SHELL COMMANDS:");
    shell.line(b"  help         show commands");
    shell.line(b"  ls [path]    list files");
    shell.line(b"  cat <file>   read file");
    shell.line(b"  hist <file>  version history");
    shell.line(b"  vN <file>    read version N");
    shell.line(b"  ps           scheduler info");
    shell.line(b"  ppo          PPO scheduler state");
    shell.line(b"  intent <cls> list by intent");
    shell.line(b"  net          network status");
    shell.line(b"  insights     latest analyst report");
    shell.line(b"  health       watchdog status");
    shell.line(b"  reward       per-core PPO reward bars");
    shell.line(b"  versions     mayafs file versions");
    shell.line(b"  watch <pid>  process stats by pid");
    shell.line(b"  tag <f> <kv> tag a file");
    shell.line(b"  clear        clear terminal");
}

fn cmd_ls(_args: &[u8], shell: &mut ShellOut) {
    if let Some(f) = MayaFile::open(b"/proc/fs", false) {
        let mut f = f;
        let mut buf = [0u8; 512];
        let n = f.read(&mut buf);
        if n > 0 {
            shell.block(&buf[..n as usize]);
            return;
        }
    }
    shell.line(b"/data/sensors");
    shell.line(b"/data/log");
    shell.line(b"/data/shared");
    shell.line(b"/sys/io/log");
}

fn cmd_cat(args: &[u8], shell: &mut ShellOut) {
    if args.is_empty() {
        shell.line(b"usage: cat <file>");
        return;
    }
    let (path_buf, path_len) = resolve_path(args);
    if let Some(f) = MayaFile::open(&path_buf[..path_len], false) {
        let mut f = f;
        let mut buf = [0u8; 256];
        let n = f.read(&mut buf);
        if n > 0 {
            shell.block(&buf[..n as usize]);
        } else {
            shell.line(b"(empty)");
        }
    } else {
        let mut line = [0u8; 160];
        let mut pos = 0usize;
        pos = push_bytes(&mut line, pos, b"not found: ");
        pos = push_bytes(&mut line, pos, args);
        shell.line(&line[..pos]);
    }
}

fn cmd_history(args: &[u8], shell: &mut ShellOut) {
    if args.is_empty() {
        shell.line(b"usage: hist <file>");
        return;
    }
    let (path_buf, path_len) = resolve_path(args);
    if let Some(f) = MayaFile::open(&path_buf[..path_len], false) {
        let mut f = f;
        let (total, oldest) = f.version_info();
        let mut header = [0u8; 160];
        let mut pos = 0usize;
        pos = push_bytes(&mut header, pos, b"versions: ");
        pos = push_u64(&mut header, pos, total as u64);
        pos = push_bytes(&mut header, pos, b" oldest: ");
        pos = push_u64(&mut header, pos, oldest as u64);
        shell.line(&header[..pos]);

        let start = if total > 5 { total - 5 } else { oldest };
        for v in start..total {
            let mut buf = [0u8; 64];
            let (n, actual) = f.read_version(v, &mut buf);
            let mut line = [0u8; 160];
            let mut lpos = 0usize;
            lpos = push_bytes(&mut line, lpos, b"  v");
            lpos = push_u64(&mut line, lpos, actual as u64);
            lpos = push_bytes(&mut line, lpos, b": ");
            if n > 0 {
                let len = (n as usize).min(32);
                lpos = push_bytes(&mut line, lpos, &buf[..len]);
            }
            shell.line(&line[..lpos]);
        }
    } else {
        shell.line(b"not found");
    }
}

fn cmd_read_version(ver_text: &[u8], args: &[u8], shell: &mut ShellOut) {
    if args.is_empty() {
        shell.line(b"usage: vN <file>");
        return;
    }
    let Some(version) = parse_u32(ver_text) else {
        shell.line(b"invalid version");
        return;
    };
    let (path_buf, path_len) = resolve_path(args);
    if let Some(f) = MayaFile::open(&path_buf[..path_len], false) {
        let mut f = f;
        let mut buf = [0u8; 128];
        let (n, actual) = f.read_version(version, &mut buf);
        let mut line = [0u8; 180];
        let mut pos = 0usize;
        pos = push_bytes(&mut line, pos, b"v");
        pos = push_u64(&mut line, pos, actual as u64);
        pos = push_bytes(&mut line, pos, b": ");
        if n > 0 {
            pos = push_bytes(&mut line, pos, &buf[..n as usize]);
        } else {
            pos = push_bytes(&mut line, pos, b"(error)");
        }
        shell.line(&line[..pos]);
    } else {
        shell.line(b"not found");
    }
}

fn cmd_ps(shell: &mut ShellOut) {
    if let Some(f) = MayaFile::open(b"/proc/sched", false) {
        let mut f = f;
        let mut buf = [0u8; 512];
        let n = f.read(&mut buf);
        if n > 0 {
            shell.block(&buf[..n as usize]);
        }
    }
}

fn cmd_ppo(shell: &mut ShellOut) {
    shell.line(b"PPO SCHEDULER");
    if let Some(f) = MayaFile::open(b"/proc/sched", false) {
        let mut f = f;
        let mut buf = [0u8; 128];
        let n = f.read(&mut buf);
        if n > 0 {
            shell.block(&buf[..n as usize]);
        }
    }
}

fn cmd_intent(args: &[u8], shell: &mut ShellOut) {
    let class = if args == b"rt" || args == b"realtime" {
        3
    } else if args == b"cpu" || args == b"compute" {
        1
    } else if args == b"io" {
        2
    } else if args == b"bg" || args == b"background" {
        4
    } else {
        shell.line(b"classes: rt cpu io bg");
        return;
    };
    let mut ids = [0u32; 16];
    let n = query_by_intent(class, &mut ids);
    let mut header = [0u8; 96];
    let mut pos = 0usize;
    pos = push_bytes(&mut header, pos, b"files (");
    pos = push_u64(&mut header, pos, n as u64);
    pos = push_bytes(&mut header, pos, b"):");
    shell.line(&header[..pos]);
    for fid in ids.iter().copied().take(n) {
        let mut line = [0u8; 64];
        let mut lpos = 0usize;
        lpos = push_bytes(&mut line, lpos, b"  fid:");
        lpos = push_u64(&mut line, lpos, fid as u64);
        shell.line(&line[..lpos]);
    }
}

fn cmd_net(shell: &mut ShellOut) {
    shell.line(b"VIRTIO-NET");
    shell.line(b"  status: online");
    shell.line(b"  port: 5555");
    let mut mac = [0u8; 6];
    MayaNet::mac(&mut mac);
    let mut line = [0u8; 96];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"  mac: ");
    for (i, byte) in mac.iter().copied().enumerate() {
        pos = push_hex_byte(&mut line, pos, byte);
        if i < 5 {
            pos = push_bytes(&mut line, pos, b":");
        }
    }
    shell.line(&line[..pos]);
}

fn cmd_insights(shell: &mut ShellOut) {
    let mut buf = [0u8; 512];
    let n = read_text_file(b"/data/insights", &mut buf);
    if n == 0 {
        shell.line(b"insights: unavailable");
        return;
    }
    let text = &buf[..n];
    shell.line(b"-- ANALYST CYCLE --------------------------------");
    let cycle = parse_u64_field(text, b"CYCLE=").unwrap_or(0);
    let mut line = [0u8; 160];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"CYCLE   ");
    pos = push_u64(&mut line, pos, cycle);
    shell.line(&line[..pos]);

    let line = format_sched_line(text);
    shell.line(trim_nul(&line));
    let line = format_ipc_line(text);
    shell.line(trim_nul(&line));
    let line = format_fs_line(text);
    shell.line(trim_nul(&line));
    let line = format_proc_line(text);
    shell.line(trim_nul(&line));
    let line = format_learn_line(text);
    shell.line(trim_nul(&line));
    shell.line(b"-----------------------------------------------");
}

fn cmd_health(shell: &mut ShellOut) {
    let mut buf = [0u8; 256];
    let n = read_text_file(b"/data/health", &mut buf);
    if n == 0 {
        shell.line(b"health: unavailable");
        return;
    }
    let text = &buf[..n];
    shell.line(b"-- WATCHDOG STATUS -----------------------------");
    let line = format_health_status_line(text);
    shell.line(trim_nul(&line));
    let line = format_health_stalls_line(text);
    shell.line(trim_nul(&line));
    let line = format_health_ipc_line(text);
    shell.line(trim_nul(&line));
    let line = format_health_learn_line(text);
    shell.line(trim_nul(&line));
    let line = format_health_analyst_line(text);
    shell.line(trim_nul(&line));
    let line = format_health_alarms_line(text);
    shell.line(trim_nul(&line));
    shell.line(b"-----------------------------------------------");
}

fn cmd_reward(shell: &mut ShellOut) {
    let mut buf = [0u8; 160];
    let n = read_text_file(b"/proc/sched", &mut buf);
    if n == 0 {
        shell.line(b"reward: unavailable");
        return;
    }
    let text = &buf[..n];
    shell.line(b"-- PPO REWARDS PER CORE ------------------------");
    for core in 0..8 {
        let mut key = [0u8; 3];
        key[0] = b'r';
        key[1] = b'0' + core as u8;
        key[2] = b':';
        let reward = parse_i32_field(text, &key).unwrap_or(0).clamp(0, 100) as u64;
        let filled = (reward / 20).min(5) as usize;
        let mut line = [0u8; 64];
        let mut pos = 0usize;
        pos = push_bytes(&mut line, pos, b"C");
        pos = push_u64(&mut line, pos, core as u64);
        pos = push_bytes(&mut line, pos, b"  ");
        for idx in 0..5 {
            pos = push_byte(&mut line, pos, if idx < filled { b'#' } else { b'.' });
        }
        pos = push_bytes(&mut line, pos, b"  ");
        pos = push_u64(&mut line, pos, reward);
        shell.line(&line[..pos]);
    }
    let mut line = [0u8; 96];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"w_sum=");
    pos = push_i32(&mut line, pos, parse_i32_field(text, b"out_w_sum:").unwrap_or(0));
    pos = push_bytes(&mut line, pos, b"  delta=");
    pos = push_signed_delta(
        &mut line,
        pos,
        parse_i32_field(text, b"delta:").unwrap_or(0),
    );
    pos = push_bytes(&mut line, pos, b"  ONLINE");
    shell.line(&line[..pos]);
    shell.line(b"-----------------------------------------------");
}

fn cmd_versions(shell: &mut ShellOut) {
    const FILES: [(&[u8], &[u8]); 6] = [
        (b"/data/sensors", b"active"),
        (b"/data/log", b"active"),
        (b"/data/shared", b"active"),
        (b"/data/insights", b"active"),
        (b"/data/health", b"active"),
        (b"/sys/realtime/sensors", b"mirror"),
    ];

    shell.line(b"-- MAYAFS VERSIONS -----------------------------");
    for (path, status) in FILES {
        if let Some(file) = MayaFile::open(path, false) {
            let (_, version) = file.stat();
            let mut line = [0u8; 160];
            let mut pos = 0usize;
            pos = push_bytes(&mut line, pos, path);
            pos = pad_to(&mut line, pos, 20);
            pos = push_bytes(&mut line, pos, b"v");
            pos = push_u64(&mut line, pos, version as u64);
            pos = pad_to(&mut line, pos, 28);
            pos = push_bytes(&mut line, pos, status);
            shell.line(&line[..pos]);
        }
    }
    shell.line(b"-----------------------------------------------");
}

fn cmd_watch(args: &[u8], shell: &mut ShellOut) {
    let Some(pid) = parse_u32(trim(args)) else {
        shell.line(b"watch: pid <N> not found");
        return;
    };
    let proc_name = known_process_name(pid as u16);
    if proc_name.is_empty() {
        let mut line = [0u8; 64];
        let mut pos = 0usize;
        pos = push_bytes(&mut line, pos, b"watch: pid ");
        pos = push_u64(&mut line, pos, pid as u64);
        pos = push_bytes(&mut line, pos, b" not found");
        shell.line(&line[..pos]);
        return;
    }
    let mut path = [0u8; 20];
    let path_len = build_proc_stats_path(&mut path, pid as u16);
    let mut buf = [0u8; 256];
    let n = read_text_file(&path[..path_len], &mut buf);
    if n == 0 {
        let mut line = [0u8; 64];
        let mut pos = 0usize;
        pos = push_bytes(&mut line, pos, b"watch: pid ");
        pos = push_u64(&mut line, pos, pid as u64);
        pos = push_bytes(&mut line, pos, b" not found");
        shell.line(&line[..pos]);
        return;
    }
    let text = &buf[..n];
    let mut header = [0u8; 160];
    let mut pos = 0usize;
    pos = push_bytes(&mut header, pos, b"-- ");
    pos = push_bytes(&mut header, pos, proc_name);
    pos = push_bytes(&mut header, pos, b" (pid ");
    pos = push_u64(&mut header, pos, pid as u64);
    pos = push_bytes(&mut header, pos, b", core ");
    pos = push_u64(&mut header, pos, known_core(pid as u16) as u64);
    pos = push_bytes(&mut header, pos, b") ----------------");
    shell.line(&header[..pos]);
    let line = format_watch_line(b"intent    ", known_intent(pid as u16));
    shell.line(trim_nul(&line));
    let line = format_watch_u64_line(b"ip_sends  ", parse_u64_field(text, b"ipc_sends:").unwrap_or(0));
    shell.line(trim_nul(&line));
    let line = format_watch_u64_line(b"ip_recvs  ", parse_u64_field(text, b"ipc_recvs:").unwrap_or(0));
    shell.line(trim_nul(&line));
    let line = format_watch_u64_line(b"alarms    ", parse_u64_field(text, b"alarms_sent:").unwrap_or(0));
    shell.line(trim_nul(&line));
    let line = format_watch_u64_line(b"acks      ", parse_u64_field(text, b"alarms_acked:").unwrap_or(0));
    shell.line(trim_nul(&line));
    let line = format_watch_u64_line(b"fs_writes ", parse_u64_field(text, b"file_writes:").unwrap_or(0));
    shell.line(trim_nul(&line));
    shell.line(b"-----------------------------------------------");
}

fn cmd_tag(args: &[u8], shell: &mut ShellOut) {
    let (file, rest) = split_first_word(args);
    if file.is_empty() || rest.is_empty() {
        shell.line(b"usage: tag <file> <key=val>");
        return;
    }
    let (path_buf, path_len) = resolve_path(file);
    if let Some(f) = MayaFile::open(&path_buf[..path_len], false) {
        if f.tag(rest) {
            shell.line(b"tagged");
        } else {
            shell.line(b"tag failed");
        }
    } else {
        shell.line(b"not found");
    }
}

fn cmd_clear(shell: &mut ShellOut) {
    shell.line(b"");
    shell.line(b"");
    shell.line(b"");
}

fn push_bytes(buf: &mut [u8], pos: usize, bytes: &[u8]) -> usize {
    let end = (pos + bytes.len()).min(buf.len());
    let count = end.saturating_sub(pos);
    buf[pos..end].copy_from_slice(&bytes[..count]);
    end
}

fn push_byte(buf: &mut [u8], pos: usize, byte: u8) -> usize {
    if pos < buf.len() {
        buf[pos] = byte;
        pos + 1
    } else {
        pos
    }
}

fn push_u64(buf: &mut [u8], pos: usize, mut value: u64) -> usize {
    if pos >= buf.len() {
        return pos;
    }
    if value == 0 {
        buf[pos] = b'0';
        return pos + 1;
    }
    let mut tmp = [0u8; 20];
    let mut len = 0usize;
    while value > 0 && len < tmp.len() {
        tmp[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    tmp[..len].reverse();
    push_bytes(buf, pos, &tmp[..len])
}

fn push_hex_byte(buf: &mut [u8], pos: usize, b: u8) -> usize {
    if pos + 1 >= buf.len() {
        return pos;
    }
    let hi = b >> 4;
    let lo = b & 0xF;
    buf[pos] = if hi < 10 { b'0' + hi } else { b'a' + hi - 10 };
    buf[pos + 1] = if lo < 10 { b'0' + lo } else { b'a' + lo - 10 };
    pos + 2
}

fn resolve_path(name: &[u8]) -> ([u8; 64], usize) {
    let mut path = [0u8; 64];
    if name.starts_with(b"/") {
        let n = name.len().min(path.len());
        path[..n].copy_from_slice(&name[..n]);
        (path, n)
    } else {
        path[..6].copy_from_slice(b"/data/");
        let n = name.len().min(path.len() - 6);
        path[6..6 + n].copy_from_slice(&name[..n]);
        (path, 6 + n)
    }
}

fn trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| b != b' ').unwrap_or(s.len());
    let end = s.iter().rposition(|&b| b != b' ').map(|i| i + 1).unwrap_or(0);
    if start >= end { &[] } else { &s[start..end] }
}

fn split_first_word(s: &[u8]) -> (&[u8], &[u8]) {
    let pos = s.iter().position(|&b| b == b' ').unwrap_or(s.len());
    let word = &s[..pos];
    let rest = if pos < s.len() { trim(&s[pos + 1..]) } else { &[] };
    (word, rest)
}

fn parse_u32(s: &[u8]) -> Option<u32> {
    let mut out = 0u32;
    if s.is_empty() {
        return None;
    }
    for &b in s {
        if !b.is_ascii_digit() {
            return None;
        }
        out = out.saturating_mul(10).saturating_add((b - b'0') as u32);
    }
    Some(out)
}

fn trim_nul(buf: &[u8]) -> &[u8] {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    &buf[..end]
}

fn parse_i32_field(buf: &[u8], key: &[u8]) -> Option<i32> {
    let start = find_subslice(buf, key)? + key.len();
    let mut idx = start;
    let mut neg = false;
    if idx < buf.len() && buf[idx] == b'-' {
        neg = true;
        idx += 1;
    }
    let mut out = 0i32;
    let mut found = false;
    while idx < buf.len() {
        let b = buf[idx];
        if !b.is_ascii_digit() {
            break;
        }
        out = out.saturating_mul(10).saturating_add((b - b'0') as i32);
        found = true;
        idx += 1;
    }
    if !found {
        None
    } else if neg {
        Some(-out)
    } else {
        Some(out)
    }
}

fn parse_u64_field(buf: &[u8], key: &[u8]) -> Option<u64> {
    let start = find_subslice(buf, key)? + key.len();
    let mut idx = start;
    let mut out = 0u64;
    let mut found = false;
    while idx < buf.len() {
        let b = buf[idx];
        if !b.is_ascii_digit() {
            break;
        }
        out = out.saturating_mul(10).saturating_add((b - b'0') as u64);
        found = true;
        idx += 1;
    }
    if found { Some(out) } else { None }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let last = haystack.len() - needle.len();
    for idx in 0..=last {
        if &haystack[idx..idx + needle.len()] == needle {
            return Some(idx);
        }
    }
    None
}

fn line_slice<'a>(buf: &'a [u8], key: &[u8]) -> &'a [u8] {
    if let Some(start) = find_subslice(buf, key) {
        let rest = &buf[start..];
        let mut end = rest.len();
        for (idx, &b) in rest.iter().enumerate() {
            if b == b'\n' {
                end = idx;
                break;
            }
        }
        &rest[..end]
    } else {
        &[]
    }
}

fn read_text_file(path: &[u8], buf: &mut [u8]) -> usize {
    if let Some(file) = MayaFile::open(path, false) {
        let n = file.read(buf);
        if n > 0 {
            return (n as usize).min(buf.len());
        }
    }
    0
}

fn format_sched_line(text: &[u8]) -> [u8; 160] {
    let mut line = [0u8; 160];
    let sched = line_slice(text, b"SCHED=");
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"SCHED   ");
    if let Some(body) = sched.strip_prefix(b"SCHED=") {
        pos = push_bytes(&mut line, pos, body);
    }
    line[pos] = 0;
    line
}

fn format_ipc_line(text: &[u8]) -> [u8; 160] {
    let mut line = [0u8; 160];
    let ipc = line_slice(text, b"IPC=");
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"IPC     ");
    if let Some(body) = ipc.strip_prefix(b"IPC=") {
        pos = push_bytes(&mut line, pos, body);
    }
    line[pos] = 0;
    line
}

fn format_fs_line(text: &[u8]) -> [u8; 160] {
    let mut line = [0u8; 160];
    let fs = line_slice(text, b"FS=");
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"FS      ");
    if let Some(body) = fs.strip_prefix(b"FS=") {
        pos = push_bytes(&mut line, pos, body);
    }
    line[pos] = 0;
    line
}

fn format_proc_line(text: &[u8]) -> [u8; 160] {
    let mut line = [0u8; 160];
    let proc = line_slice(text, b"PROC=");
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"PROC    ");
    if let Some(body) = proc.strip_prefix(b"PROC=") {
        pos = push_bytes(&mut line, pos, body);
    }
    line[pos] = 0;
    line
}

fn format_learn_line(text: &[u8]) -> [u8; 160] {
    let mut line = [0u8; 160];
    let learn = line_slice(text, b"LEARN=");
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"LEARN   ");
    if let Some(body) = learn.strip_prefix(b"LEARN=") {
        pos = push_bytes(&mut line, pos, body);
    }
    line[pos] = 0;
    line
}

fn format_health_status_line(text: &[u8]) -> [u8; 128] {
    let mut line = [0u8; 128];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"STATUS  ");
    pos = push_bytes(&mut line, pos, line_value(text, b"WATCH="));
    line[pos] = 0;
    line
}

fn format_health_stalls_line(text: &[u8]) -> [u8; 128] {
    let mut line = [0u8; 128];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"STALLS  ");
    pos = push_bytes(&mut line, pos, line_value(text, b"STALLS="));
    line[pos] = 0;
    line
}

fn format_health_ipc_line(text: &[u8]) -> [u8; 128] {
    let mut line = [0u8; 128];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"IPC     ");
    pos = push_bytes(
        &mut line,
        pos,
        if line_value(text, b"IPC_ALERT=") == b"yes" { b"alert" } else { b"healthy" },
    );
    line[pos] = 0;
    line
}

fn format_health_learn_line(text: &[u8]) -> [u8; 128] {
    let mut line = [0u8; 128];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"LEARN   ");
    pos = push_bytes(
        &mut line,
        pos,
        if line_value(text, b"LEARN_ALERT=") == b"yes" { b"alert" } else { b"healthy" },
    );
    line[pos] = 0;
    line
}

fn format_health_analyst_line(text: &[u8]) -> [u8; 128] {
    let mut line = [0u8; 128];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"ANALYST ");
    pos = push_bytes(
        &mut line,
        pos,
        if line_value(text, b"ANALYST_OK=") == b"yes" {
            b"heartbeat ok"
        } else {
            b"stalled"
        },
    );
    line[pos] = 0;
    line
}

fn format_health_alarms_line(text: &[u8]) -> [u8; 128] {
    let mut line = [0u8; 128];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, b"ALARMS  ");
    pos = push_bytes(&mut line, pos, line_value(text, b"ALARMS_SENT="));
    pos = push_bytes(&mut line, pos, b" sent this session");
    line[pos] = 0;
    line
}

fn line_value<'a>(buf: &'a [u8], key: &[u8]) -> &'a [u8] {
    let line = line_slice(buf, key);
    if line.len() >= key.len() {
        &line[key.len()..]
    } else {
        &[]
    }
}

fn format_watch_line(prefix: &[u8], value: &[u8]) -> [u8; 64] {
    let mut line = [0u8; 64];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, prefix);
    pos = push_bytes(&mut line, pos, value);
    line[pos] = 0;
    line
}

fn format_watch_u64_line(prefix: &[u8], value: u64) -> [u8; 64] {
    let mut line = [0u8; 64];
    let mut pos = 0usize;
    pos = push_bytes(&mut line, pos, prefix);
    pos = push_u64(&mut line, pos, value);
    line[pos] = 0;
    line
}

fn push_i32(buf: &mut [u8], pos: usize, value: i32) -> usize {
    if value < 0 {
        let pos = push_byte(buf, pos, b'-');
        push_u64(buf, pos, value.unsigned_abs() as u64)
    } else {
        push_u64(buf, pos, value as u64)
    }
}

fn push_signed_delta(buf: &mut [u8], pos: usize, value: i32) -> usize {
    let pos = if value > 0 { push_byte(buf, pos, b'+') } else { pos };
    push_i32(buf, pos, value)
}

fn pad_to(buf: &mut [u8], mut pos: usize, target: usize) -> usize {
    while pos < target && pos < buf.len() {
        buf[pos] = b' ';
        pos += 1;
    }
    pos
}

fn build_proc_stats_path(buf: &mut [u8; 20], pid: u16) -> usize {
    buf[..6].copy_from_slice(b"/proc/");
    let mut pos = 6usize;
    pos = push_u64(buf, pos, pid as u64);
    buf[pos] = b'/';
    pos += 1;
    buf[pos..pos + 5].copy_from_slice(b"stats");
    pos + 5
}

fn known_core(pid: u16) -> u8 {
    match pid {
        11 | 12 => 0,
        10 | 13 | 14 | 16 => 1,
        4 => 2,
        5 => 3,
        6 | 15 => 4,
        7 => 5,
        8 => 6,
        9 => 7,
        _ => 0,
    }
}

fn known_process_name(pid: u16) -> &'static [u8] {
    match pid {
        4 => b"compute",
        5 => b"io",
        6 => b"background_task",
        7 => b"matrix_multiply",
        8 => b"net_parser",
        9 => b"sort_suite",
        10 => b"mrt_hello",
        11 => b"mrt_producer",
        12 => b"mrt_consumer",
        13 => b"mrt_logger",
        14 => b"mrt_shell",
        15 => b"mrt_analyst",
        16 => b"mrt_watchdog",
        _ => b"",
    }
}

fn known_intent(pid: u16) -> &'static [u8] {
    match pid {
        11 | 12 | 10 | 16 => b"RealTime",
        4 | 7 | 9 => b"Compute",
        5 | 8 | 13 | 14 => b"IO",
        6 | 15 => b"Background",
        _ => b"Unknown",
    }
}
