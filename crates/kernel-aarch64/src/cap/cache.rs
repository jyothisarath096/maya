use super::{auth_token, pac_available, CapToken};

#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct PerCoreCapCache {
    pub hi: [u64; 8],
    pub lo: [u64; 8],
    pub valid: [bool; 8],
    pub cursor: usize,
}

impl PerCoreCapCache {
    pub const fn new() -> Self {
        Self {
            hi: [0; 8],
            lo: [0; 8],
            valid: [false; 8],
            cursor: 0,
        }
    }
}

static mut PER_CORE_CACHE: [PerCoreCapCache; 8] = [PerCoreCapCache::new(); 8];

fn current_core_id() -> usize {
    let mpidr: u64;
    unsafe {
        core::arch::asm!(
            "mrs {mpidr}, mpidr_el1",
            mpidr = out(reg) mpidr,
            options(nomem, nostack, preserves_flags)
        );
    }
    (mpidr & 0xFF) as usize
}

pub fn cache_lookup(token: CapToken) -> Option<()> {
    crate::uart_print!("");
    let core = current_core_id().min(7);
    let cache = unsafe { &PER_CORE_CACHE[core] };
    for i in 0..8 {
        if cache.valid[i] && cache.lo[i] == token.lo() {
            let cached = CapToken::from_parts(cache.hi[i], cache.lo[i]);
            let authed = if pac_available() {
                auth_token(cached).ok()?
            } else {
                cached
            };
            unsafe {
                core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
            }
            if authed.generation() == token.generation() || pac_available() {
                return Some(());
            }
            unsafe {
                core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
            }
        }
    }
    None
}

pub fn cache_insert(token: CapToken) {
    let core = current_core_id().min(7);
    let cache = unsafe { &mut PER_CORE_CACHE[core] };
    let slot = cache.cursor % 8;
    cache.hi[slot] = token.hi();
    cache.lo[slot] = token.lo();
    cache.valid[slot] = true;
    cache.cursor = (cache.cursor + 1) % 8;
}

pub fn cap_cache_flush_local() {
    let core = current_core_id().min(7);
    let cache = unsafe { &mut PER_CORE_CACHE[core] };
    for i in 0..8 {
        cache.valid[i] = false;
    }
}
