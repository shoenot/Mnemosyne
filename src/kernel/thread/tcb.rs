use core::ptr::null_mut;

use crate::kernel::thread::schedule::get_new_tid;

#[derive(PartialEq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[derive(PartialEq)]
pub enum ThreadPriority {
    Idle,
    Low,
    Medium,
    High,
    Maximum,
}

#[repr(C)]
#[derive(PartialEq)]
pub struct ThreadControlBlock {
    pub thread_id: usize,
    pub state: ThreadState,
    pub priority: ThreadPriority,
    pub wake_time: usize,
    pub total_runtime: usize,
    pub stack_ptr: usize,
    pub stack_base: usize,
    pub extended_context: *mut u8,
    pub next: *mut ThreadControlBlock,
}

impl ThreadControlBlock {
    pub fn init(&mut self, stack_ptr: usize, stack_base: usize, fpu_ptr: *mut u8) {
        self.thread_id = get_new_tid();
        self.state = ThreadState::Ready;
        self.priority = ThreadPriority::Medium;
        self.total_runtime = 0;
        self.stack_ptr = stack_ptr;
        self.stack_base = stack_base;
        self.extended_context = fpu_ptr;
        self.next = null_mut();
    }
}

unsafe extern "sysv64" {
    pub fn switch_threads_avx(
        old_stack_ptr: *mut usize, new_stack_ptr: usize, old_extended_context: *mut u8, new_extended_context: *const u8,
    );

    pub fn switch_threads_legacy(
        old_stack_ptr: *mut usize, new_stack_ptr: usize, old_extended_context: *mut u8, new_extended_context: *const u8,
    );
}

unsafe impl Send for ThreadControlBlock {}
unsafe impl Sync for ThreadControlBlock {}
