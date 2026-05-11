use core::{
    arch::asm,
    ptr::null_mut,
};

use super::schedule::*;

#[repr(C, align(16))]
pub struct ThreadContext {
    pub rax: usize,
    pub rbx: usize,
    pub rcx: usize,
    pub rdx: usize,
    pub rsi: usize,
    pub rdi: usize,
    pub rbp: usize,
    pub r8: usize,
    pub r9: usize,
    pub r10: usize,
    pub r11: usize,
    pub r12: usize,
    pub r13: usize,
    pub r14: usize,
    pub r15: usize,

    pub interrupt_number: u64,
    pub error_code: u64,

    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

impl ThreadContext {
    pub fn zero_gp(&mut self) {
        self.rax = 0;
        self.rbx = 0;
        self.rcx = 0;
        self.rdx = 0;
        self.rsi = 0;
        self.rdi = 0;
        self.rbp = 0;
        self.r8 = 0;
        self.r9 = 0;
        self.r10 = 0;
        self.r11 = 0;
        self.r12 = 0;
        self.r13 = 0;
        self.r14 = 0;
        self.r15 = 0;
    }
}

#[repr(C)]
pub struct SwitchContext {
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub rbp: usize,
    pub rbx: usize,
    pub rip: usize,
}

impl SwitchContext {
    pub fn init(&mut self) {
        self.r12 = 0;
        self.r13 = 0;
        self.r14 = 0;
        self.r15 = 0;
        self.rbx = 0;
        self.rbp = 0;
    }
}

#[repr(C, align(16))]
pub struct ExtendedContext {
    pub fcw: u16,
    pub fsw: u16,
    pub ftw: u16,
    pub fop: u16,
    pub f_rip: u64,
    pub f_rdp: u64,
    pub mxcsr: u32,
    pub mxcsr_mask: u32,
    pub mmx_regs: [[u8; 16]; 8],
    pub sse_regs: [[u8; 16]; 16],
    pub reserved: [u8; 96],
}

impl ExtendedContext {
    pub const fn new() -> Self {
        Self {
            fcw: 0,
            fsw: 0,
            ftw: 0,
            fop: 0,
            f_rip: 0,
            f_rdp: 0,
            mxcsr: 0,
            mxcsr_mask: 0,
            mmx_regs: [[0; 16]; 8],
            sse_regs: [[0; 16]; 16],
            reserved: [0; 96],
        }
    }

    pub unsafe fn init_default_state(&mut self) {
        unsafe {
            asm!("fninit",
                "fxsave64 [{}]",
                in(reg) self,
                options(nostack, preserves_flags));
        }
        self.mxcsr = 0x1F80;
    }
}

#[repr(C)]
pub struct ThreadControlBlock {
    pub thread_id: usize,
    pub state: ThreadState,
    pub priority: ThreadPriority,
    pub total_runtime: usize,
    pub stack_ptr: usize,
    pub stack_base: usize,
    pub extended_context: *mut ExtendedContext,
    pub next: *mut ThreadControlBlock,
}

impl ThreadControlBlock {
    pub fn init(&mut self, stack_ptr: usize, stack_base: usize, fpu_ptr: *mut ExtendedContext) {
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
    pub fn switch_threads(
        old_stack_ptr: *mut usize, new_stack_ptr: usize, old_extended_context: *mut ExtendedContext,
        new_extended_context: *const ExtendedContext,
    );
}

unsafe impl Send for ThreadControlBlock {}
unsafe impl Sync for ThreadControlBlock {}
