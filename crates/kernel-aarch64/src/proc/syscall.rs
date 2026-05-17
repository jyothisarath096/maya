#![allow(dead_code)]

extern crate alloc;

use crate::{
    arch::exceptions::SyscallFrame,
    cap::{self, CapToken, IntentClass, ResourceType, Rights},
    io::{
        audit::MediatorDecision,
        syscall::{IoEventKind, IoRequest},
    },
    ipc::channel::{self},
    proc,
};
use alloc::vec;

#[inline(always)]
fn current_core_pid() -> u16 {
    let pid: u64;
    unsafe {
        core::arch::asm!(
            "mrs {pid}, tpidr_el0",
            pid = out(reg) pid,
            options(nomem, nostack)
        );
    }
    pid as u16
}

pub unsafe fn read_user_byte(user_ptr: u64) -> u8 {
    let val: u64;
    core::arch::asm!(
        "ldtrb {val:w}, [{ptr}]",
        val = out(reg) val,
        ptr = in(reg) user_ptr,
        options(nostack)
    );
    val as u8
}

pub unsafe fn write_user_byte(user_ptr: u64, val: u8) {
    core::arch::asm!(
        "sttrb {val:w}, [{ptr}]",
        val = in(reg) val as u64,
        ptr = in(reg) user_ptr,
        options(nostack)
    );
}

pub unsafe fn read_user_bytes(user_ptr: u64, dst: &mut [u8]) {
    for (index, byte) in dst.iter_mut().enumerate() {
        *byte = read_user_byte(user_ptr + index as u64);
    }
}

pub fn dispatch_core(nr: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64) -> (i64, i64) {
    match nr {
        0x00 => (0, 0),
        0x01 => (0, 0),
        0x02 => (-1, 0),
        0x03 => (a0 as i64, 0),
        0x04 => (0, 0),
        _ => (-1, 0),
    }
}

pub fn sys_yield(frame: &mut SyscallFrame) {
    let pid = current_core_pid();
    proc::save_syscall_frame(pid, frame);

    // Drive scheduler ticks since IRQs may be masked during SVC handling.
    crate::arch::timer::handle_tick();

    let next_pid = crate::sched::queue::choose_next_process(Some(pid)).unwrap_or(pid);
    if next_pid == pid {
        return;
    }

    let Some((ttbr0, asid)) = proc::get_process_ttbr0(next_pid) else {
        return;
    };
    crate::memory::vmm::set_user_table(ttbr0, asid);

    let next_frame = proc::get_process_frame(next_pid);
    unsafe {
        core::arch::asm!(
            "msr tpidr_el0, {pid}",
            "msr tpidr_el1, {v}",
            pid = in(reg) next_pid as u64,
            v = in(reg) next_frame as u64,
            options(nomem, nostack, preserves_flags)
        );
    }

    if let Some(next_sp_el0) = proc::load_syscall_frame(next_pid, frame) {
        unsafe {
            core::arch::asm!(
                "msr sp_el0, {v}",
                "isb",
                v = in(reg) next_sp_el0,
                options(nomem, nostack, preserves_flags)
            );
        }
        frame.sp_el0 = next_sp_el0;
    }

    proc::set_current_pid(next_pid);
    let core_id = crate::arch::cpu::current_core_id() as usize;
    proc::set_current_proc(core_id, next_pid);

}

pub fn dispatch_cap(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> (i64, i64) {
    match nr {
        0x10 => {
            let pid = current_core_pid();
            let resource = match a0 as u16 {
                1 => ResourceType::Memory,
                2 => ResourceType::Channel,
                3 => ResourceType::Process,
                4 => ResourceType::Interrupt,
                5 => ResourceType::Intent,
                6 => ResourceType::Telemetry,
                7 => ResourceType::Network,
                10 => ResourceType::File,
                _ => ResourceType::Crypto,
            };
            match cap::create(
                pid,
                resource,
                a1 as u32,
                Rights(a2 as u16),
                a3 as u16,
                IntentClass::Unknown,
            ) {
                Ok(token) => (token.lo() as i64, token.hi() as i64),
                Err(_) => (-1, 0),
            }
        }
        0x11 => {
            let token = CapToken::from_parts(a1, a0);
            if cap::revoke(token).is_ok() { (0, 0) } else { (-1, 0) }
        }
        0x12 => {
            let token = CapToken::from_parts(a1, a0);
            match cap::delegate(token, a2 as u16, Rights(a3 as u16)) {
                Ok(child) => (child.lo() as i64, child.hi() as i64),
                Err(_) => (-1, 0),
            }
        }
        0x13 => {
            let token = CapToken::from_parts(a1, a0);
            if cap::check_right(token, Rights(a2 as u16)).is_ok() { (0, 0) } else { (-1, 0) }
        }
        _ => (-1, 0),
    }
}

pub fn dispatch_ipc(nr: u64, a0: u64, a1: u64, a2: u64, _a3: u64) -> (i64, i64) {
    match nr {
        0x20 => {
            let pid = current_core_pid();
            match channel::create_channel(pid, a0 as u16) {
                Ok((sender_cap, _receiver_cap)) => (sender_cap.lo() as i64, sender_cap.hi() as i64),
                Err(_) => (-1, 0),
            }
        }
        0x21 => {
            let pid = current_core_pid();
            let found_token = cap::find_by_lo(a0);
            let token = found_token.unwrap_or_else(|| CapToken::from_parts(0, a0));
            let buf_ptr = a1;
            let len = (a2 as usize).min(52);
            let mut payload = [0u8; 52];
            unsafe {
                read_user_bytes(buf_ptr, &mut payload[..len]);
            }
            if channel::send_payload(token, pid, &payload).is_ok() { (0, 0) } else { (-1, 0) }
        }
        0x22 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            let buf_ptr = a1;
            let req_len = (a2 as usize).min(52);
            match channel::recv_to_user(token, buf_ptr, req_len) {
                Ok(n) => (n as i64, 0),
                Err(_) => (-1, 0),
            }
        }
        0x23 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(a1, a0));
            match channel::recv(token) {
                Ok(msg) => (i64::from_le_bytes(msg.payload[..8].try_into().unwrap()), 0),
                Err(_) => (-1, 0),
            }
        }
        0x123 => {
            let pid = current_core_pid();
            match channel::lookup_recv_cap(pid) {
                Some(token) => (token.lo() as i64, token.hi() as i64),
                None => (-1, 0),
            }
        }
        0x124 => {
            let pid = current_core_pid();
            match channel::lookup_send_cap(pid) {
                Some(token) => (token.lo() as i64, token.hi() as i64),
                None => (-1, 0),
            }
        }
        _ => (-1, 0),
    }
}

pub fn dispatch_fs(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> (i64, i64) {
    let pid = current_core_pid();
    let now_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());

    match nr {
        0x30 => {
            let path_len = (a1 as usize).min(64);
            let mut path = [0u8; 64];
            unsafe {
                read_user_bytes(a0, &mut path[..path_len]);
            }
            let kind = if (a3 & 1) != 0 {
                IoEventKind::FileCreate
            } else {
                IoEventKind::FileOpen
            };
            let request = IoRequest {
                kind,
                path: Some(path),
                path_len,
                size: 0,
                offset: 0,
                cap_token: None,
            };
            if matches!(
                crate::io::mediator::mediate(pid, &request).decision,
                MediatorDecision::Block
            ) {
                return (-1, 0);
            }

            let fid = match crate::fs::lookup_path(&path[..path_len]) {
                Some(fid) => fid,
                None if (a3 & 1) != 0 => {
                    let creator_class = proc::get_process_intent_class(pid) as u8;
                    let Some(fid) = crate::fs::alloc_file(pid, creator_class, now_ns) else {
                        return (-1, 0);
                    };
                    if crate::fs::insert_path(&path[..path_len], fid, false).is_err() {
                        return (-1, 0);
                    }
                    let intent_name = file_intent_name(creator_class);
                    crate::fs::store::tag_file(fid, b"intent", intent_name).ok();
                    let mut pid_buf = [0u8; 5];
                    let pid_str = u16_to_str(pid, &mut pid_buf);
                    crate::fs::store::tag_file(fid, b"creator", pid_str).ok();

                    let fname_start = path[..path_len]
                        .iter()
                        .rposition(|&b| b == b'/')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let fname_len = path_len.saturating_sub(fname_start);
                    let mut sys_path = [0u8; 64];
                    sys_path[..5].copy_from_slice(b"/sys/");
                    let iname_len = intent_name.len();
                    sys_path[5..5 + iname_len].copy_from_slice(intent_name);
                    sys_path[5 + iname_len] = b'/';
                    let sys_path_len = 6 + iname_len + fname_len;
                    if sys_path_len <= sys_path.len() {
                        sys_path[6 + iname_len..sys_path_len]
                            .copy_from_slice(&path[fname_start..path_len]);
                        crate::fs::namespace::insert_path(&sys_path[..sys_path_len], fid, false).ok();
                    }
                    fid
                }
                None => return (-1, 0),
            };

            let rights = if (a3 & 1) != 0 {
                Rights(Rights::READ.0 | Rights::WRITE.0 | Rights::GRANT.0)
            } else {
                Rights::READ
            };
            match cap::create(
                pid,
                ResourceType::File,
                fid.0,
                rights,
                a2 as u16,
                proc::get_process_intent_class(pid),
            ) {
                Ok(token) => (token.lo() as i64, token.hi() as i64),
                Err(_) => (-1, 0),
            }
        }
        0x31 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            if cap::check_right_as(token, Rights::READ, pid).is_err() {
                return (-1, 0);
            }
            let request = IoRequest {
                kind: IoEventKind::FileRead,
                path: None,
                path_len: 0,
                size: a2 as usize,
                offset: a3 as usize,
                cap_token: Some(token),
            };
            if matches!(
                crate::io::mediator::mediate(pid, &request).decision,
                MediatorDecision::Block
            ) {
                return (-1, 0);
            }
            let fid = match cap::get_resource_id(token) {
                Ok(id) => crate::fs::FileId(id),
                Err(_) => return (-1, 0),
            };
            let limit = (a2 as usize).min(crate::fs::store::MAX_FILE_SIZE);
            if crate::fs::store::is_virtual_file(fid) {
                let mut kbuf = [0u8; 256];
                match crate::fs::store::read_virtual(fid, pid, &mut kbuf) {
                    Ok(n) => {
                        let copy = n.min(limit);
                        for (i, b) in kbuf[..copy].iter().copied().enumerate() {
                            unsafe {
                                write_user_byte(a1 + i as u64, b);
                            }
                        }
                        (copy as i64, 0)
                    }
                    Err(_) => (-1, 0),
                }
            } else {
                let mut tmp = vec![0u8; limit];
                let n = match crate::fs::read_file_copy(fid, pid, now_ns, &mut tmp) {
                    Ok(n) => n,
                    Err(_) => return (-1, 0),
                };
                for (i, b) in tmp[..n].iter().copied().enumerate() {
                    unsafe {
                        write_user_byte(a1 + i as u64, b);
                    }
                }
                (n as i64, 0)
            }
        }
        0x32 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            if cap::check_right_as(token, Rights::WRITE, pid).is_err() {
                return (-1, 0);
            }
            let request = IoRequest {
                kind: IoEventKind::FileWrite,
                path: None,
                path_len: 0,
                size: a2 as usize,
                offset: a3 as usize,
                cap_token: Some(token),
            };
            if matches!(
                crate::io::mediator::mediate(pid, &request).decision,
                MediatorDecision::Block
            ) {
                return (-1, 0);
            }
            let fid = match cap::get_resource_id(token) {
                Ok(id) => crate::fs::FileId(id),
                Err(_) => return (-1, 0),
            };
            let len = (a2 as usize).min(crate::fs::store::MAX_FILE_SIZE);
            let mut tmp = vec![0u8; len];
            unsafe {
                read_user_bytes(a1, &mut tmp);
            }
            match crate::fs::write_file(fid, &tmp, pid, now_ns) {
                Ok(version) => (len as i64, version as i64),
                Err(_) => (-1, 0),
            }
        }
        0x33 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            if cap::check_right_as(token, Rights::READ, pid).is_err() {
                return (-1, 0);
            }
            let fid = match cap::get_resource_id(token) {
                Ok(id) => crate::fs::FileId(id),
                Err(_) => return (-1, 0),
            };
            match crate::fs::store::stat_file(fid) {
                Ok((size, versions)) => (size as i64, versions as i64),
                Err(_) => (-1, 0),
            }
        }
        0x34 => {
            let path_len = (a1 as usize).min(128);
            let mut path = [0u8; 128];
            unsafe {
                read_user_bytes(a0, &mut path[..path_len]);
            }
            if crate::fs::mkdir(&path[..path_len]).is_ok() {
                (0, 0)
            } else {
                (-1, 0)
            }
        }
        0x35 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            match cap::revoke(token) {
                Ok(()) => (0, 0),
                Err(_) => (0, 0),
            }
        }
        0x36 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            if cap::check_right_as(token, Rights::READ, pid).is_err() {
                return (-1, 0);
            }
            let fid = match cap::get_resource_id(token) {
                Ok(id) => crate::fs::FileId(id),
                Err(_) => return (-1, 0),
            };
            let buf_ptr = a1;
            let buf_len = (a2 as usize).min(512);
            let version_num = a3 as u32;
            let mut kbuf = [0u8; 512];
            match crate::fs::store::read_file_version(fid, version_num, pid, now_ns, &mut kbuf[..buf_len]) {
                Ok((n, actual_ver)) => {
                    if n >= 5 {
                        unsafe {
                            let uart = 0xFFFF_0000_0900_0000u64 as *mut u32;
                            for _ in 0..3 {
                                uart.write_volatile(b'V' as u32);
                                uart.write_volatile((b'0' + (actual_ver % 10) as u8) as u32);
                                uart.write_volatile(b':' as u32);
                                for &byte in &kbuf[..5] {
                                    uart.write_volatile(byte as u32);
                                }
                                uart.write_volatile(b'\n' as u32);
                            }
                        }
                    }
                    for (i, b) in kbuf[..n].iter().copied().enumerate() {
                        unsafe {
                            write_user_byte(buf_ptr + i as u64, b);
                        }
                    }
                    (n as i64, actual_ver as i64)
                }
                Err(_) => (-1, 0),
            }
        }
        0x37 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            match cap::delegate(token, a1 as u16, Rights(a2 as u16)) {
                Ok(delegated) => (delegated.lo() as i64, delegated.hi() as i64),
                Err(_) => (-1, 0),
            }
        }
        0x38 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            match cap::revoke(token) {
                Ok(()) => (0, 0),
                Err(_) => (-1, 0),
            }
        }
        0x39 => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            if cap::check_right_as(token, Rights::READ, pid).is_err() {
                return (-1, 0);
            }
            let fid = match cap::get_resource_id(token) {
                Ok(id) => crate::fs::FileId(id),
                Err(_) => return (-1, 0),
            };
            let count = crate::fs::store::file_version_count(fid);
            let oldest = crate::fs::store::file_oldest_version(fid);
            (count as i64, oldest as i64)
        }
        0x3A => {
            let token = cap::find_by_lo(a0).unwrap_or_else(|| CapToken::from_parts(0, a0));
            if cap::check_right_as(token, Rights::WRITE, pid).is_err() {
                return (-1, 0);
            }
            let fid = match cap::get_resource_id(token) {
                Ok(id) => crate::fs::FileId(id),
                Err(_) => return (-1, 0),
            };
            let kv_len = (a2 as usize).min(64);
            let mut kv = [0u8; 64];
            unsafe {
                read_user_bytes(a1, &mut kv[..kv_len]);
            }
            let Some(sep) = kv[..kv_len].iter().position(|&b| b == b'=') else {
                return (-1, 0);
            };
            match crate::fs::store::tag_file(fid, &kv[..sep], &kv[sep + 1..kv_len]) {
                Ok(()) => (0, 0),
                Err(_) => (-1, 0),
            }
        }
        0x3B => {
            let intent = a0 as u8;
            let buf_ptr = a1;
            let max = (a2 as usize).min(32);
            let mut ids = [crate::fs::FileId(0); 32];
            let count = crate::fs::store::query_files_by_intent(intent, &mut ids[..max]);
            for (i, file_id) in ids[..count].iter().enumerate() {
                let v = file_id.0;
                unsafe {
                    write_user_byte(buf_ptr + i as u64 * 4, (v & 0xFF) as u8);
                    write_user_byte(buf_ptr + i as u64 * 4 + 1, ((v >> 8) & 0xFF) as u8);
                    write_user_byte(buf_ptr + i as u64 * 4 + 2, ((v >> 16) & 0xFF) as u8);
                    write_user_byte(buf_ptr + i as u64 * 4 + 3, ((v >> 24) & 0xFF) as u8);
                }
            }
            (count as i64, 0)
        }
        _ => (-1, 0),
    }
}

pub fn dispatch_net(nr: u64, a0: u64, a1: u64, a2: u64, _a3: u64) -> (i64, i64) {
    match nr {
        0x40 => {
            let len = (a1 as usize).min(1400);
            let mut payload = [0u8; 1400];
            unsafe {
                read_user_bytes(a0, &mut payload[..len]);
            }
            let pkt = crate::net::udp::UdpPacket::new(
                crate::net::driver::get_mac(),
                [0xFF; 6],
                [10, 0, 2, 15],
                [10, 0, 2, 2],
                7777,
                a2 as u16,
                &payload[..len],
            );
            if crate::net::driver::send_packet(pkt.as_bytes()) {
                (len as i64, 0)
            } else {
                (-1, 0)
            }
        }
        0x41 => {
            let mut kbuf = [0u8; 1500];
            let n = crate::net::driver::recv_packet(&mut kbuf);
            if n == 0 {
                return (0, 0);
            }
            let copy = n.min(a1 as usize);
            for (i, b) in kbuf[..copy].iter().copied().enumerate() {
                unsafe {
                    write_user_byte(a0 + i as u64, b);
                }
            }
            (copy as i64, 0)
        }
        0x42 => {
            let mac = crate::net::driver::get_mac();
            for (i, b) in mac.iter().copied().enumerate() {
                unsafe {
                    write_user_byte(a0 + i as u64, b);
                }
            }
            (6, 0)
        }
        _ => (-1, 0),
    }
}

pub fn dispatch_input(nr: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> (i64, i64) {
    match nr {
        0x50 => {
            let ch = crate::input::keyboard::read_key()
                .or_else(crate::uart::read_byte_nonblocking)
                .unwrap_or(0);
            (ch as i64, 0)
        }
        _ => (-1, 0),
    }
}

fn file_intent_name(intent_class: u8) -> &'static [u8] {
    match intent_class {
        1 => b"compute",
        2 => b"io",
        3 => b"realtime",
        4 => b"background",
        5 => b"system",
        _ => b"unknown",
    }
}

fn u16_to_str(v: u16, buf: &mut [u8; 5]) -> &[u8] {
    let mut tmp = [0u8; 5];
    let mut len = 0usize;
    let mut n = v as u32;
    if n == 0 {
        tmp[0] = b'0';
        len = 1;
    } else {
        while n > 0 && len < 5 {
            tmp[len] = b'0' + (n % 10) as u8;
            n /= 10;
            len += 1;
        }
        tmp[..len].reverse();
    }
    buf[..len].copy_from_slice(&tmp[..len]);
    &buf[..len]
}

pub fn dispatch_intent(
    frame: &mut SyscallFrame,
    nr: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    _a3: u64,
) -> (i64, i64) {
    match nr {
        0x80 => sys_intent_reg(a0, a1, a2),
        0x88 => sys_telemetry(a0, a1),
        0x89 => {
            crate::proc::inject::complete_injection(frame, a0 as u16, a1, a2);
            let result = sys_inject_return(a0, a1, a2);
            sys_yield(frame);
            result
        }
        _ => (-1, 0),
    }
}

pub fn sys_inject_return(intent_id: u64, return_val_lo: u64, return_val_hi: u64) -> (i64, i64) {
    let pid = current_core_pid();
    let now_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    proc::set_injection_result(pid, intent_id as u16, return_val_lo, return_val_hi, now_ns);

    crate::uart_print!("INJ_RET,");
    crate::uart_print_usize!(pid as usize);
    crate::uart_print!(",");
    crate::uart_print_usize!(intent_id as usize);
    crate::uart_print!(",");
    crate::uart_print_hex!(return_val_lo);
    crate::uart_print!("\n");

    (0, 0)
}

pub fn dispatch_injection(
    frame: &mut SyscallFrame,
    target_pid: u64,
    intent_id: u64,
    arg0: u64,
    arg1: u64,
) -> (i64, i64) {
    let req = proc::inject::InjectionRequest {
        target_pid: target_pid as u16,
        intent_id: intent_id as u16,
        args: [arg0, arg1, 0, 0, 0, 0, 0, 0],
        timeout_ns: 0,
    };

    match proc::inject::inject_function_call(frame, req) {
        Ok(()) => (0, 0),
        Err(_) => (-1, 0),
    }
}

pub fn dispatch_io(nr: u64, cap_lo: u64, cap_hi: u64, arg2: u64, arg3: u64) -> (i64, i64) {
    match nr {
        0x110 => return sys_alloc(cap_lo, cap_hi, arg2),
        0x111 => return sys_free(cap_lo, cap_hi),
        0x114 => return sys_chan_pipe(cap_lo),
        0x115 => return sys_get_console_cap(),
        0x116 => return sys_chan_grant(cap_lo, cap_hi, arg2),
        0x123 => {
            let core_id = crate::arch::cpu::current_core_id() as usize;
            let pid = proc::current_process_for_core(core_id);
            return match channel::lookup_recv_cap(pid) {
                Some(token) => (token.lo() as i64, token.hi() as i64),
                None => (-1, 0),
            };
        }
        0x124 => {
            let core_id = crate::arch::cpu::current_core_id() as usize;
            let pid = proc::current_process_for_core(core_id);
            return match channel::lookup_send_cap(pid) {
                Some(token) => (token.lo() as i64, token.hi() as i64),
                None => (-1, 0),
            };
        }
        _ => {}
    }

    let pid = current_core_pid();
    let token = if nr == 0x101 {
        proc::get_process_console_cap(pid).unwrap_or_else(|| {
            cap::find_by_lo(cap_lo).unwrap_or_else(|| CapToken::from_parts(cap_hi, cap_lo))
        })
    } else {
        CapToken::from_parts(cap_hi, cap_lo)
    };

    let kind = match nr {
        0x100 => IoEventKind::FileRead,
        0x101 => IoEventKind::FileWrite,
        0x102 => IoEventKind::FileOpen,
        0x103 => IoEventKind::NetworkSend,
        0x104 => IoEventKind::NetworkRecv,
        0x105 => IoEventKind::MemoryMap,
        _ => return (-1, 0),
    };

    let request = IoRequest {
        kind,
        path: None,
        path_len: 0,
        size: arg2 as usize,
        offset: arg3 as usize,
        cap_token: Some(token),
    };

    let result = crate::io::mediator::mediate(pid, &request);
    match result.decision {
        MediatorDecision::Block => (-38, 0),
        MediatorDecision::Flag | MediatorDecision::Allow => {
            dispatch_io_handler(nr, token, pid, cap_hi, arg2, arg3)
        }
    }
}

fn dispatch_io_handler(
    nr: u64,
    _token: CapToken,
    _pid: u16,
    cap_hi: u64,
    arg2: u64,
    _arg3: u64,
) -> (i64, i64) {
    match nr {
        0x101 => {
            let buf_ptr = cap_hi;
            let len = (arg2 as usize).min(256);
            crate::uart::with_lock(|| {
                let uart = 0xFFFF_0000_0900_0000u64 as *mut u32;
                for index in 0..len {
                    let byte = unsafe { read_user_byte(buf_ptr + index as u64) };
                    unsafe {
                        uart.write_volatile(byte as u32);
                    }
                }
            });
            (len as i64, 0)
        }
        _ => (0, 0),
    }
}

fn sys_alloc(size: u64, alignment: u64, intent_class: u64) -> (i64, i64) {
    let pid = current_core_pid();
    let class = intent_class_from_u16(intent_class as u16);
    match proc::allocate_process_memory(pid, size as usize, alignment as usize, class) {
        Ok((vaddr, token)) => {
            emit_allocator_telemetry(pid, 0x110, class);
            (vaddr as i64, token.lo() as i64)
        }
        Err(_) => (-1, 0),
    }
}

fn sys_free(cap_lo: u64, cap_hi: u64) -> (i64, i64) {
    let pid = current_core_pid();
    let _ = cap_hi;
    match proc::free_process_memory_by_lo(pid, cap_lo) {
        Ok(()) => {
            emit_allocator_telemetry(pid, 0x111, proc::get_process_intent_class(pid));
            (0, 0)
        }
        Err(_) => (-1, 0),
    }
}

fn sys_chan_pipe(buffer_size: u64) -> (i64, i64) {
    let pid = current_core_pid();
    let _ = buffer_size;
    match channel::create_channel(pid, pid) {
        Ok((_read_cap, write_cap)) => {
            emit_allocator_telemetry(pid, 0x114, proc::get_process_intent_class(pid));
            (write_cap.lo() as i64, write_cap.hi() as i64)
        }
        Err(_) => (-1, 0),
    }
}

fn sys_get_console_cap() -> (i64, i64) {
    let pid = current_core_pid();
    match proc::get_process_console_cap(pid) {
        Some(token) => (token.lo() as i64, token.hi() as i64),
        None => (-1, 0),
    }
}

fn sys_chan_grant(cap_lo: u64, cap_hi: u64, target_pid: u64) -> (i64, i64) {
    let token = cap::find_by_lo(cap_lo).unwrap_or_else(|| CapToken::from_parts(cap_hi, cap_lo));
    match cap::delegate(
        token,
        target_pid as u16,
        Rights(Rights::READ.0 | Rights::WRITE.0),
    ) {
        Ok(child) => (child.lo() as i64, child.hi() as i64),
        Err(_) => (-1, 0),
    }
}

fn emit_allocator_telemetry(pid: u16, intent_id: u16, class: IntentClass) {
    let now_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    crate::sched::queue::update_process_intent(pid, intent_id, now_ns);
    proc::update_process_intent_class(pid, class);
    let _ = (pid, intent_id, class, now_ns);
}

fn intent_class_from_u16(value: u16) -> IntentClass {
    match value {
        1 => IntentClass::Compute,
        2 => IntentClass::IO,
        3 => IntentClass::RealTime,
        4 => IntentClass::Background,
        5 => IntentClass::System,
        _ => IntentClass::Unknown,
    }
}

fn sys_intent_reg(name_ptr: u64, name_len: u64, intent_class: u64) -> (i64, i64) {
    sys_intent_reg_inner(name_ptr, name_len, intent_class)
}

fn sys_intent_reg_inner(name_ptr: u64, name_len: u64, intent_class: u64) -> (i64, i64) {
    let mut name_buf = [0u8; 64];
    let len = (name_len as usize).min(64);
    let pid = current_core_pid();
    unsafe { read_user_bytes(name_ptr, &mut name_buf[..len]); }

    let class = intent_class_from_u16(intent_class as u16);
    let intent_id = proc::allocate_intent_id(pid);
    let token = match cap::create(
        pid,
        ResourceType::Intent,
        intent_id as u32,
        Rights(Rights::EXECUTE.0 | Rights::OBSERVE.0 | Rights::INTENT_CALL.0),
        intent_id,
        class,
    ) {
        Ok(token) => token,
        Err(_) => return (-1, 0),
    };
    if proc::register_process_intent(pid, intent_id, token).is_err() {
        return (-1, 0);
    }

    proc::update_process_intent_class(pid, class);
    crate::sched::queue::update_process_intent(
        pid,
        intent_id,
        crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct()),
    );

    let _ = name_buf;
    (token.lo() as i64, token.hi() as i64)
}

fn sys_telemetry(intent_id: u64, data_ptr: u64) -> (i64, i64) {
    sys_telemetry_inner(intent_id, data_ptr)
}

fn sys_telemetry_inner(intent_id: u64, _data_ptr: u64) -> (i64, i64) {
    let pid = current_core_pid();
    let core_id = crate::arch::cpu::current_core_id();
    let now_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    crate::sched::queue::update_process_intent(pid, intent_id as u16, now_ns);

    if let Some(token) = proc::get_process_intent_cap(pid, intent_id as u16) {
        if let Ok(class) = cap::get_intent_class(token) {
            proc::update_process_intent_class(pid, class);
        }
    }

    let _ = (core_id, now_ns);

    (0, 0)
}
