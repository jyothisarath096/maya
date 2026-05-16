#![allow(dead_code)]

use alloc::vec::Vec;

use crate::{
    KernelError,
    cap::{self, CapToken, ResourceType, Rights},
    sync::TicketLock,
};

#[derive(Debug, Clone, Copy)]
pub struct Message {
    pub sender_pid: u16,
    pub payload: [u8; 56],
    pub cap_transfer: Option<CapToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelState {
    Empty,
    HasMessage,
    Closed,
}

struct Channel {
    state: ChannelState,
    message: Option<Message>,
}

struct ChannelTable {
    channels: Vec<Channel>,
}

static CHANNEL_TABLE: TicketLock<Option<ChannelTable>> = TicketLock::new(None);
const APIC_BASE: u64 = 0xFEE00000;
const APIC_ICR_LOW: u32 = 0x300;
const APIC_ICR_HIGH: u32 = 0x310;

pub fn init() {
    *CHANNEL_TABLE.lock() = Some(ChannelTable {
        channels: Vec::new(),
    });
}

pub fn create_channel() -> Result<(CapToken, CapToken), KernelError> {
    let channel_id = {
        let mut guard = CHANNEL_TABLE.lock();
        let table = guard.as_mut().ok_or(KernelError::IpcNotInitialized)?;
        table.channels.push(Channel {
            state: ChannelState::Empty,
            message: None,
        });
        table.channels.len() - 1
    };

    let sender_cap = cap::create(0, ResourceType::Channel, channel_id as u32, Rights::WRITE)?;
    let receiver_cap = cap::create(0, ResourceType::Channel, channel_id as u32, Rights::READ)?;
    Ok((sender_cap, receiver_cap))
}

pub fn send(sender_cap: CapToken, msg: Message) -> Result<(), KernelError> {
    cap::check_right(sender_cap, Rights::WRITE)?;
    let channel_id = cap::get_resource_id(sender_cap)? as usize;

    let mut guard = CHANNEL_TABLE.lock();
    let table = guard.as_mut().ok_or(KernelError::IpcNotInitialized)?;
    let channel = table
        .channels
        .get_mut(channel_id)
        .ok_or(KernelError::IpcInvalidChannel)?;

    match channel.state {
        ChannelState::HasMessage => return Err(KernelError::IpcChannelFull),
        ChannelState::Closed => return Err(KernelError::IpcChannelClosed),
        ChannelState::Empty => {}
    }

    channel.message = Some(msg);
    channel.state = ChannelState::HasMessage;
    Ok(())
}

pub fn recv(receiver_cap: CapToken) -> Result<Message, KernelError> {
    cap::check_right(receiver_cap, Rights::READ)?;
    let channel_id = cap::get_resource_id(receiver_cap)? as usize;

    let mut guard = CHANNEL_TABLE.lock();
    let table = guard.as_mut().ok_or(KernelError::IpcNotInitialized)?;
    let channel = table
        .channels
        .get_mut(channel_id)
        .ok_or(KernelError::IpcInvalidChannel)?;

    match channel.state {
        ChannelState::Empty => return Err(KernelError::IpcChannelEmpty),
        ChannelState::Closed => return Err(KernelError::IpcChannelClosed),
        ChannelState::HasMessage => {}
    }

    let message = channel.message.take().ok_or(KernelError::IpcChannelEmpty)?;
    channel.state = ChannelState::Empty;
    Ok(message)
}

pub fn send_cross_core(
    channel: CapToken,
    msg: Message,
    target_core: u8,
) -> Result<(), KernelError> {
    send(channel, msg)?;
    unsafe {
        let dest = (target_core as u32) << 24;
        write_apic(APIC_ICR_HIGH, dest);
        write_apic(APIC_ICR_LOW, 0x0000_4021);
    }
    Ok(())
}

unsafe fn write_apic(offset: u32, value: u32) {
    let ptr = (APIC_BASE + offset as u64) as *mut u32;
    ptr.write_volatile(value);
}
