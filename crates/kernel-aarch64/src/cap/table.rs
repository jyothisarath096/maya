use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU32, Ordering},
};

macro_rules! bti_c {
    () => {
        unsafe {
            core::arch::asm!(
                ".inst 0xD503245F",
                options(nostack, nomem, preserves_flags)
            )
        }
    };
}

#[repr(C)]
pub struct RawSpinLock<T> {
    lock: AtomicU32,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for RawSpinLock<T> {}

impl<T> RawSpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            lock: AtomicU32::new(0),
            value: UnsafeCell::new(value),
        }
    }

    #[inline(never)]
    pub fn lock(&self) -> RawSpinLockGuard<'_, T> {
        bti_c!();
        loop {
            match self
                .lock
                .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(_) => {
                    while self.lock.load(Ordering::Relaxed) != 0 {
                        core::hint::spin_loop();
                    }
                }
            }
        }
        RawSpinLockGuard { lock: self }
    }

    #[inline(never)]
    pub fn try_lock(&self) -> Option<RawSpinLockGuard<'_, T>> {
        bti_c!();
        self.lock
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| RawSpinLockGuard { lock: self })
    }
}

pub struct RawSpinLockGuard<'a, T> {
    lock: &'a RawSpinLock<T>,
}

impl<T> Deref for RawSpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for RawSpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for RawSpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.store(0, Ordering::Release);
    }
}
