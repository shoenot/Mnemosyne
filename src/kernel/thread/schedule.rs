#![allow(dead_code)]

use alloc::alloc::{
    Layout,
    alloc,
};
use core::{
    mem::size_of,
    ptr::{
        copy_nonoverlapping,
        null_mut,
    },
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use super::{
    ThreadError,
    switch::*,
};
use crate::{
    arch::x86_64::interrupts::gdt::{
        KERNEL_CS,
        KERNEL_SS,
    },
    kernel::lock::TicketLock,
};

pub static SCHEDULER: TicketLock<SchedulerState> = TicketLock::new(SchedulerState::new());

pub static GLOBAL_TID: AtomicUsize = AtomicUsize::new(0);

pub static DEFAULT_EXTENDED_CONTEXT: TicketLock<Option<ExtendedContext>> = TicketLock::new(None);

const RFLAGS_IF: u64 = 0x202; // bit 9 is interrupt enable and bit 1 is always 1 (reserved)

#[derive(PartialEq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

pub enum ThreadPriority {
    Idle,
    Low,
    Medium,
    High,
    Maximum,
}

pub struct SchedulerState {
    ready_queue_head: *mut ThreadControlBlock,
    ready_queue_tail: *mut ThreadControlBlock,
    current_thread: *mut ThreadControlBlock,
    idle_thread: *mut ThreadControlBlock,
}

unsafe impl Send for SchedulerState {}
unsafe impl Sync for SchedulerState {}

pub fn get_new_tid() -> usize { GLOBAL_TID.fetch_add(1, Ordering::Relaxed) }

pub fn init_clean_fpu() {
    let mut clean_state = ExtendedContext::new();
    unsafe {
        clean_state.init_default_state();
    }
    let mut dec = DEFAULT_EXTENDED_CONTEXT.lock();
    *dec = Some(clean_state);
}

impl SchedulerState {
    pub const fn new() -> Self {
        SchedulerState { ready_queue_head: null_mut(), ready_queue_tail: null_mut(), current_thread: null_mut(), idle_thread: null_mut() }
    }

    pub fn push(&mut self, thread: *mut ThreadControlBlock) {
        unsafe {
            (*thread).next = null_mut(); // ensure new thread isn't linked to anything else
            if self.ready_queue_tail.is_null() {
                self.ready_queue_head = thread;
                self.ready_queue_tail = thread;
            } else {
                (*self.ready_queue_tail).next = thread;
                self.ready_queue_tail = thread;
            }
        }
    }

    pub fn pop(&mut self) -> *mut ThreadControlBlock {
        unsafe {
            if self.ready_queue_head.is_null() {
                return null_mut();
            }

            let ret = self.ready_queue_head;
            self.ready_queue_head = (*ret).next;
            if self.ready_queue_head.is_null() {
                self.ready_queue_tail = null_mut();
            }
            (*ret).next = null_mut(); // ensure ret thread isn't linked ot anything else
            ret
        }
    }

    pub fn spawn(&mut self, entry_point: usize) -> Result<(), ThreadError> {
        let stack_size = 4096 * 4;
        // alloc memory for structs
        let tcb_layout = Layout::new::<ThreadControlBlock>();
        let stack_layout = Layout::from_size_align(stack_size, 4096)?;
        let fpu_layout = Layout::from_size_align(size_of::<ExtendedContext>(), 16)?;

        let tcb_ptr = unsafe { alloc(tcb_layout) as *mut ThreadControlBlock };
        let stack_base = unsafe { alloc(stack_layout) as usize };
        let fpu_ptr = unsafe { alloc(fpu_layout) as *mut ExtendedContext };

        // init extended context state
        {
            let dec = DEFAULT_EXTENDED_CONTEXT.lock();
            let default_fpu_ref = dec.as_ref().expect("Clean FPU not initialized");
            unsafe { copy_nonoverlapping(default_fpu_ref as *const ExtendedContext, fpu_ptr, 1) };
        }

        let stack_top = stack_base + stack_size;
        let context_addr = stack_top - size_of::<ThreadContext>();
        let context_addr = context_addr & !0xF; // align to 16 bytes
        let context = unsafe { &mut *(context_addr as *mut ThreadContext) };

        context.zero_gp();
        context.instruction_pointer = entry_point as u64;
        context.stack_pointer = stack_top as u64;
        context.code_segment = KERNEL_CS;
        context.stack_segment = KERNEL_SS;
        context.cpu_flags = RFLAGS_IF;

        let switch_addr = context_addr - size_of::<SwitchContext>();
        let switch_context = unsafe { &mut *(switch_addr as *mut SwitchContext) };
        switch_context.init();

        unsafe extern "C" {
            fn thread_entry_stub();
        }
        switch_context.rip = (thread_entry_stub as *const ()) as usize;

        // init TCB
        unsafe {
            (*tcb_ptr).init(switch_addr, stack_base, fpu_ptr);
        }

        // push new tcb to queue
        self.push(tcb_ptr);

        Ok(())
    }

    pub fn schedule(&mut self) {
        let next_thread = self.pop();
        if next_thread.is_null() {
            return;
        }

        let prev_thread = self.current_thread;
        if !prev_thread.is_null() {
            unsafe {
                if (*prev_thread).state == ThreadState::Running {
                    (*prev_thread).state = ThreadState::Ready;
                    self.push(prev_thread);
                }
            }
        }

        self.current_thread = next_thread;
        unsafe {
            (*next_thread).state = ThreadState::Running;
        }

        if !prev_thread.is_null() {
            unsafe {
                switch_threads(
                    &mut (*prev_thread).stack_ptr as *mut usize,
                    (*next_thread).stack_ptr,
                    (*prev_thread).extended_context,
                    (*next_thread).extended_context,
                );
            }
        } else {
            let mut dummy_stack_ptr = 0usize;
            let mut dummy_fpu = ExtendedContext::new();
            unsafe {
                switch_threads(
                    &mut dummy_stack_ptr as *mut usize,
                    (*next_thread).stack_ptr,
                    &mut dummy_fpu as *mut ExtendedContext,
                    (*next_thread).extended_context,
                );
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn unlock_scheduler() {
    unsafe {
        SCHEDULER.force_unlock();
    }
}
