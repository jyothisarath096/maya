use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU32, Ordering},
};

pub struct TicketLock<T> {
    next_ticket: AtomicU32,
    now_serving: AtomicU32,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for TicketLock<T> {}
unsafe impl<T: Send> Send for TicketLock<T> {}

impl<T> TicketLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            next_ticket: AtomicU32::new(0),
            now_serving: AtomicU32::new(0),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> TicketLockGuard<'_, T> {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        while self.now_serving.load(Ordering::Acquire) != ticket {
            spin_loop();
        }
        TicketLockGuard { lock: self }
    }

    fn unlock(&self) {
        self.now_serving.fetch_add(1, Ordering::Release);
    }
}

pub struct TicketLockGuard<'a, T> {
    lock: &'a TicketLock<T>,
}

impl<T> Deref for TicketLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for TicketLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for TicketLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.unlock();
    }
}
