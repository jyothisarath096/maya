#![allow(dead_code)]

#[derive(Debug, Clone, Copy)]
pub enum PerfEvent {
    CpuCycles,
    Instructions,
    CacheMisses,
    BranchMisses,
    TlbMisses,
}

pub struct PerfCounter {
    event: PerfEvent,
    start: u64,
    enabled: bool,
}

impl PerfCounter {
    pub fn new(event: PerfEvent) -> Self {
        Self {
            event,
            start: 0,
            enabled: false,
        }
    }

    pub fn start(&mut self) {
        unsafe {
            let event_select = match self.event {
                PerfEvent::CpuCycles => 0x003C00,
                PerfEvent::Instructions => 0x00C000,
                PerfEvent::CacheMisses => 0x412E00,
                PerfEvent::BranchMisses => 0x04C500,
                PerfEvent::TlbMisses => 0x2008500,
            };
            let ctrl = event_select | (1 << 16) | (1 << 17) | (1 << 22);
            wrmsr(0x186, ctrl);
            wrmsr(0xC1, 0);
            self.start = rdpmc(0);
            self.enabled = true;
        }
    }

    pub fn stop(&mut self) -> u64 {
        if !self.enabled {
            return 0;
        }
        self.enabled = false;
        unsafe { rdpmc(0).saturating_sub(self.start) }
    }
}

unsafe fn wrmsr(msr: u32, val: u64) {
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") val as u32,
        in("edx") (val >> 32) as u32,
        options(nomem, nostack, preserves_flags)
    );
}

unsafe fn rdpmc(counter: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdpmc",
        in("ecx") counter,
        out("eax") lo,
        out("edx") hi,
        options(nomem, nostack, preserves_flags)
    );
    ((hi as u64) << 32) | lo as u64
}

pub fn rdtsc() -> u64 {
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
        ((hi as u64) << 32) | lo as u64
    }
}
