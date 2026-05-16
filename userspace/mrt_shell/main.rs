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
