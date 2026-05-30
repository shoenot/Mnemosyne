use alloc::vec::Vec;
use core::ptr::null_mut;
use core::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering,
};

use crate::core::thread::ThreadControlBlock;
use crate::core::thread::dispatch::wake_thread;
use crate::impl_queue_methods;

#[derive(Debug)]
pub struct WaitQueue {
    pub queue_length: AtomicUsize,
    head: *mut ThreadControlBlock,
    tail: *mut ThreadControlBlock,
}

unsafe impl Send for WaitQueue {}

impl WaitQueue {
    pub const fn new() -> Self { Self { queue_length: AtomicUsize::new(0), head: null_mut(), tail: null_mut() } }
}

impl_queue_methods!(WaitQueue, ThreadControlBlock, head, tail);

pub struct WakeToken {
    pub fired: AtomicBool,
    pub thread: *mut ThreadControlBlock,
}

unsafe impl Send for WakeToken {}
unsafe impl Sync for WakeToken {}

impl WakeToken {
    pub fn new(thread: *mut ThreadControlBlock) -> Self { Self { fired: AtomicBool::new(false), thread } }
}

#[derive(Debug)]
pub struct MultiWakeQueue {
    tokens: Vec<*mut WakeToken>,
}

unsafe impl Send for MultiWakeQueue {}

impl MultiWakeQueue {
    pub fn new() -> Self { Self { tokens: Vec::new() } }

    pub fn push(&mut self, token: *mut WakeToken) { self.tokens.push(token); }

    pub fn wake_all(&mut self) {
        for token_ptr in self.tokens.drain(..) {
            unsafe {
                let token = &*token_ptr;
                if token.fired.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                    wake_thread(token.thread);
                }
            }
        }
    }

    pub fn remove(&mut self, token: *mut WakeToken) { self.tokens.retain(|&x| x != token); }
}
