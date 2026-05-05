use core::{
    arch::asm,
    sync::atomic::{
        AtomicBool, Ordering::{Acquire, Release, Relaxed},
    },
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};
use core::hint::spin_loop;

#[inline]
fn interrupts_enabled() -> bool {
    let rflags: usize;
    unsafe {
        asm!(
            "pushfq",
            "popq {}",
            out(reg) rflags,
            options(att_syntax, nomem, preserves_flags)
        )
    }
    (rflags & (1 << 9)) != 0
}

#[inline]
fn enable_interrupts() {
    unsafe { asm!("sti", options(att_syntax, nomem, nostack)) };
}

#[inline]
fn disable_interrupts() {
    unsafe { asm!("cli", options(att_syntax, nomem, nostack)) };
}

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T> Sync for SpinLock<T> where T: Send {}
unsafe impl<T> Send for SpinLock<T> where T: Send {}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self { 
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let interrupts_state = interrupts_enabled();
        if interrupts_state { disable_interrupts(); }

        loop {
            if !self.locked.swap(true, Acquire) {
                break;
            }

            while self.locked.load(Relaxed) {
                spin_loop();
            }
        }

        SpinLockGuard {
            lock: self,
            interrupts_state,
        }
    }
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    interrupts_state: bool,
}

unsafe impl<T> Send for SpinLockGuard<'_, T> where T: Send {}
unsafe impl<T> Sync for SpinLockGuard<'_, T> where T: Sync {}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Release);
        if self.interrupts_state {
            enable_interrupts();
        }
    }
}
