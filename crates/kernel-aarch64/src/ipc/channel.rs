#![allow(dead_code)]

use crate::{
    cap::{self, CapToken, IntentClass, ResourceType, Rights},
    cap::table::RawSpinLock,
    KernelError,
};

const MAX_CHANNELS: usize = 64;
const CH_EMPTY: u32 = 0;
const CH_HAS_MSG: u32 = 1;
const CH_CLOSED: u32 = 2;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Message {
    pub sender_pid: u16,
    pub intent_id: u16,
    pub intent_class: IntentClass,
    pub _pad: [u8; 2],
    pub payload: [u8; 52],
    pub cap_transfer: Option<CapToken>,
}

#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct Channel {
    state: u32,
    _pad: u32,
    has_message: bool,
    _pad2: [u8; 7],
    message: Message,
    sender_pid: u16,
    recv_pid: u16,
}

struct ChannelTable {
    channels: [Channel; MAX_CHANNELS],
    count: usize,
}

const EMPTY_MESSAGE: Message = Message {
    sender_pid: 0,
    intent_id: 0,
    intent_class: IntentClass::Unknown,
    _pad: [0; 2],
    payload: [0; 52],
    cap_transfer: None,
};

const EMPTY_CHANNEL: Channel = Channel {
    state: CH_EMPTY,
    _pad: 0,
    has_message: false,
    _pad2: [0; 7],
    message: EMPTY_MESSAGE,
    sender_pid: 0,
    recv_pid: 0,
};

const EMPTY_TABLE: ChannelTable = ChannelTable {
    channels: [EMPTY_CHANNEL; MAX_CHANNELS],
    count: 0,
};

static CHANNEL_TABLE: RawSpinLock<ChannelTable> = RawSpinLock::new(EMPTY_TABLE);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PayloadKind {
    Regular,
    Alarm,
    Ack,
}

fn classify_payload(payload: &[u8; 52]) -> PayloadKind {
    let flags = payload[7];
    if flags == 0x80 {
        PayloadKind::Ack
    } else if flags & 0x02 != 0 {
        PayloadKind::Alarm
    } else {
        PayloadKind::Regular
    }
}

pub fn init() {
    let mut table = CHANNEL_TABLE.lock();
    *table = EMPTY_TABLE;
}

pub fn handle_ipc_sgi() {
    let core_id = crate::arch::cpu::current_core_id() as usize;
    if crate::proc::current_process_for_core(core_id) != 0 {
        unsafe {
            core::arch::asm!("sev", options(nomem, nostack, preserves_flags));
        }
        return;
    }

    let next_pid = crate::sched::queue::choose_next_process(None).unwrap_or(0);
    if next_pid == 0 {
        return;
    }

    let Some((entry, stack, ttbr0, asid)) = crate::proc::get_process_launch_params(next_pid) else {
        return;
    };

    let frame_ptr = crate::proc::get_process_frame(next_pid);
    if frame_ptr.is_null() {
        return;
    }

    crate::proc::set_current_process_for_core(core_id, next_pid);
    crate::proc::set_current_pid(next_pid);
    unsafe {
        core::arch::asm!(
            "msr tpidr_el0, {pid}",
            "msr tpidr_el1, {frame}",
            "isb",
            pid = in(reg) next_pid as u64,
            frame = in(reg) frame_ptr as u64,
            options(nomem, nostack)
        );
        crate::arch::timer::enable_ap_timer();
        crate::proc::jump_to_el0(entry, stack, ttbr0, asid);
    }
}

#[inline(never)]
fn cas_channel_state(state_ptr: *mut u32, expected: u32, new: u32) -> bool {
    let mut old = expected;
    unsafe {
        core::arch::asm!(
            "cas {old:w}, {new:w}, [{ptr}]",
            old = inout(reg) old,
            new = in(reg) new,
            ptr = in(reg) state_ptr,
            options(nostack)
        );
    }
    old == expected
}

pub fn create_channel(sender_pid: u16, receiver_pid: u16) -> Result<(CapToken, CapToken), KernelError> {
    let mut table = CHANNEL_TABLE.lock();
    for index in 0..MAX_CHANNELS {
        if table.channels[index].sender_pid == 0 && table.channels[index].recv_pid == 0 {
            table.channels[index] = Channel {
                state: CH_EMPTY,
                _pad: 0,
                has_message: false,
                _pad2: [0; 7],
                message: EMPTY_MESSAGE,
                sender_pid,
                recv_pid: receiver_pid,
            };
            table.count = table.count.max(index + 1);
            drop(table);

            let send_cap = cap::create(
                sender_pid,
                ResourceType::Channel,
                index as u32,
                Rights(Rights::WRITE.0 | Rights::GRANT.0),
                0,
                IntentClass::Unknown,
            )?;
            let recv_cap = cap::create(
                receiver_pid,
                ResourceType::Channel,
                index as u32,
                Rights::READ,
                0,
                IntentClass::Unknown,
            )?;
            return Ok((send_cap, recv_cap));
        }
    }
    Err(KernelError::IpcInvalidChannel)
}

pub fn lookup_recv_cap(receiver_pid: u16) -> Option<CapToken> {
    let table = CHANNEL_TABLE.lock();
    let channel_id = table
        .channels
        .iter()
        .enumerate()
        .find(|(_, channel)| channel.recv_pid == receiver_pid && channel.sender_pid != 0)
        .map(|(index, _)| index as u32)?;
    drop(table);
    cap::find_channel_cap(receiver_pid, channel_id)
}

pub fn lookup_send_cap(sender_pid: u16) -> Option<CapToken> {
    let table = CHANNEL_TABLE.lock();
    let channel_id = table
        .channels
        .iter()
        .enumerate()
        .find(|(_, channel)| channel.sender_pid == sender_pid)
        .map(|(index, _)| index as u32)?;
    drop(table);
    cap::find_channel_cap(sender_pid, channel_id)
}

pub fn send_payload(
    channel: CapToken,
    sender_pid: u16,
    payload: &[u8; 52],
) -> Result<(), KernelError> {
    cap::check_right(channel, Rights::WRITE)?;
    let channel_id = cap::get_resource_id(channel)? as usize;
    if channel_id >= MAX_CHANNELS {
        return Err(KernelError::IpcInvalidChannel);
    }

    let receiver_pid = {
        let table = CHANNEL_TABLE.lock();
        let channel_ref = &table.channels[channel_id];
        if channel_ref.state == CH_CLOSED {
            return Err(KernelError::IpcChannelClosed);
        }
        let state_ptr = core::ptr::addr_of!(channel_ref.state) as *mut u32;
        if !cas_channel_state(state_ptr, CH_EMPTY, CH_HAS_MSG) {
            return Err(KernelError::IpcChannelFull);
        }
        channel_ref.recv_pid
    };

    let payload_kind = classify_payload(payload);

    let msg = Message {
        sender_pid,
        intent_id: 0,
        intent_class: IntentClass::Unknown,
        _pad: [0; 2],
        payload: *payload,
        cap_transfer: None,
    };

    let mut table = CHANNEL_TABLE.lock();
    table.channels[channel_id].message = msg;
    table.channels[channel_id].has_message = true;
    table.channels[channel_id].sender_pid = sender_pid;
    drop(table);

    crate::sched::queue::update_process_ipc_stats(sender_pid, 1, 0);
    match payload_kind {
        PayloadKind::Alarm => {
            let _ = crate::sched::queue::update_process_alarm_stats(sender_pid, 1, 0);
        }
        PayloadKind::Ack => {
            let _ = crate::sched::queue::update_process_alarm_stats(sender_pid, 0, 1);
        }
        PayloadKind::Regular => {}
    }

    crate::sched::queue::update_process_intent(
        receiver_pid,
        0,
        crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct()),
    );

    unsafe {
        core::arch::asm!("dsb st", "sev", options(nomem, nostack, preserves_flags));
    }

    Ok(())
}

pub fn recv_to_user(
    receiver_cap: CapToken,
    buf_ptr: u64,
    len: usize,
) -> Result<usize, KernelError> {
    cap::check_right(receiver_cap, Rights::READ)?;
    let channel_id = cap::get_resource_id(receiver_cap)? as usize;
    if channel_id >= MAX_CHANNELS {
        return Err(KernelError::IpcInvalidChannel);
    }
    let mut table = CHANNEL_TABLE.lock();
    if table.channels[channel_id].state == CH_CLOSED {
        return Err(KernelError::IpcChannelClosed);
    }
    if !table.channels[channel_id].has_message {
        return Err(KernelError::IpcChannelEmpty);
    }
    let state_ptr = core::ptr::addr_of!(table.channels[channel_id].state) as *mut u32;
    if !cas_channel_state(state_ptr, CH_HAS_MSG, CH_EMPTY) {
        return Err(KernelError::IpcChannelEmpty);
    }
    let copy_len = len.min(52);
    let msg_ptr = core::ptr::addr_of!(table.channels[channel_id].message.payload) as *const u8;
    table.channels[channel_id].has_message = false;
    drop(table);
    unsafe {
        for i in 0..copy_len {
            let b = *msg_ptr.add(i);
            crate::proc::syscall::write_user_byte(buf_ptr + i as u64, b);
        }
    }
    crate::sched::queue::update_process_ipc_stats(receiver_cap.owner_pid(), 0, 1);
    Ok(copy_len)
}

pub fn send(channel: CapToken, mut msg: Message) -> Result<(), KernelError> {
    cap::check_right(channel, Rights::WRITE)?;
    let channel_id = cap::get_resource_id(channel)? as usize;
    if channel_id >= MAX_CHANNELS {
        return Err(KernelError::IpcInvalidChannel);
    }

    let sender_pid = channel.owner_pid();
    let receiver_pid = {
        let table = CHANNEL_TABLE.lock();
        let channel_ref = &table.channels[channel_id];
        if channel_ref.state == CH_CLOSED {
            return Err(KernelError::IpcChannelClosed);
        }
        let state_ptr = core::ptr::addr_of!(channel_ref.state) as *mut u32;
        if !cas_channel_state(state_ptr, CH_EMPTY, CH_HAS_MSG) {
            return Err(KernelError::IpcChannelFull);
        }
        channel_ref.recv_pid
    };

    if let Some(token) = msg.cap_transfer {
        msg.cap_transfer = Some(validate_cap_transfer(token, sender_pid, receiver_pid)?);
    }

    let mut table = CHANNEL_TABLE.lock();
    table.channels[channel_id].message = msg;
    table.channels[channel_id].has_message = true;
    table.channels[channel_id].sender_pid = sender_pid;
    drop(table);

    crate::sched::queue::update_process_ipc_stats(sender_pid, 1, 0);

    crate::sched::queue::update_process_intent(
        receiver_pid,
        msg.intent_id,
        crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct()),
    );

    unsafe {
        core::arch::asm!("dsb st", "sev", options(nomem, nostack, preserves_flags));
    }

    Ok(())
}

pub fn send_cross_core(channel: CapToken, msg: Message, target_core: u8) -> Result<(), KernelError> {
    send(channel, msg)?;
    unsafe {
        let sgir = (0xFFFF_0000_0800_0000u64 + 0xF00) as *mut u32;
        let target_list = 1u32 << target_core;
        let sgir_val = (0b10 << 24) | (target_list << 16) | 1;
        sgir.write_volatile(sgir_val);
    }
    Ok(())
}

pub fn recv(receiver_cap: CapToken) -> Result<Message, KernelError> {
    cap::check_right(receiver_cap, Rights::READ)?;
    let channel_id = cap::get_resource_id(receiver_cap)? as usize;
    if channel_id >= MAX_CHANNELS {
        return Err(KernelError::IpcInvalidChannel);
    }

    let mut table = CHANNEL_TABLE.lock();
    if table.channels[channel_id].state == CH_CLOSED {
        return Err(KernelError::IpcChannelClosed);
    }
    if table.channels[channel_id].state == CH_EMPTY {
        return Err(KernelError::IpcChannelEmpty);
    }
    if !table.channels[channel_id].has_message {
        return Err(KernelError::IpcChannelEmpty);
    }
    let state_ptr = core::ptr::addr_of!(table.channels[channel_id].state) as *mut u32;
    if !cas_channel_state(state_ptr, CH_HAS_MSG, CH_EMPTY) {
        return Err(KernelError::IpcChannelEmpty);
    }
    let msg = table.channels[channel_id].message;
    table.channels[channel_id].has_message = false;
    let receiver_pid = receiver_cap.owner_pid();
    drop(table);
    crate::sched::queue::update_process_ipc_stats(receiver_pid, 0, 1);
    Ok(msg)
}

pub fn recv_blocking(receiver_cap: CapToken, timeout_ns: Option<u64>) -> Result<Message, KernelError> {
    cap::check_right(receiver_cap, Rights::READ)?;
    let deadline = timeout_ns.map(|ns| {
        crate::arch::timer::read_cntpct()
            + ns.saturating_mul(crate::arch::timer::cntfrq()) / 1_000_000_000
    });

    loop {
        match recv(receiver_cap) {
            Ok(msg) => return Ok(msg),
            Err(KernelError::IpcChannelEmpty) => {}
            Err(error) => return Err(error),
        }

        if let Some(deadline) = deadline {
            if crate::arch::timer::read_cntpct() >= deadline {
                return Err(KernelError::IpcTimeout);
            }
        }

        unsafe {
            core::arch::asm!("wfe", options(nomem, nostack, preserves_flags));
        }
    }
}

pub fn send_to_intent(_sender_cap: CapToken, _intent_id: u16, _payload: [u8; 52]) -> Result<(), KernelError> {
    Err(KernelError::IpcNoRoute)
}

fn validate_cap_transfer(token: CapToken, sender_pid: u16, receiver_pid: u16) -> Result<CapToken, KernelError> {
    cap::check_right_as(token, Rights::GRANT, sender_pid)
        .map_err(|_| KernelError::IpcCapTransferDenied)?;
    let receiver_rights = Rights(cap::get_rights(token)? & !Rights::GRANT.0);
    cap::delegate(token, receiver_pid, receiver_rights).map_err(|_| KernelError::IpcCapTransferDenied)
}
