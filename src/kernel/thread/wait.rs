use core::ptr::null_mut;
use crate::kernel::thread::ThreadControlBlock;

pub struct WaitQueue {
    head: *mut ThreadControlBlock,
    tail: *mut ThreadControlBlock,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self { head: null_mut(), tail: null_mut() }
    }

    pub fn push(&mut self, tcb: *mut ThreadControlBlock) {
        unsafe {
            (*tcb).next = null_mut();
            if self.tail.is_null() {
                self.head = tcb;
                self.tail = tcb;
            } else {
                (*self.tail).next = tcb;
                self.tail = tcb;
            }
        }
    }

    pub fn pop(&mut self) -> *mut ThreadControlBlock {
        if self.head.is_null() {
            return null_mut();
        }
        unsafe {
            let ret = self.head;
            self.head = (*ret).next;
            if self.head.is_null() {
                self.tail = null_mut();
            }
            (*ret).next = null_mut();
            ret
        }
    }
}
