use core::{
    arch::asm,
    sync::atomic::Ordering,
    ptr::copy_nonoverlapping,
};
use alloc::alloc::{
    alloc,
    Layout,
};

use crate::{
    arch::x86_64::{
        cpu::fpu::*,
        task::context::*,
        interrupts::gdt::{
            KERNEL_CS, 
            KERNEL_SS,
        },
    },
    kernel::thread::{
        ThreadControlBlock,
        ThreadPriority,
        schedule::RFLAGS_IF,
    },
    klogln,
};

fn idle_loop() -> ! {
    unsafe {
        klogln!("Nothing to do. Entering idle loop.");
        loop {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

pub fn init_idle_thread() -> *mut ThreadControlBlock {
    let stack_size = 4096;
    let fpu_size = FPU_CXT_SIZE.load(Ordering::Relaxed);

    let tcb_layout = Layout::new::<ThreadControlBlock>();
    let stack_layout = Layout::from_size_align(stack_size, 4096).ok().unwrap();
    let tcb_ptr = unsafe { alloc(tcb_layout) as *mut ThreadControlBlock };
    let stack_base = unsafe { alloc(stack_layout) as usize };

    let fpu_ptr = if USE_XSAVE.load(Ordering::Relaxed) {
        gen_avx_dummy_fpu().ok().unwrap()
    } else {
        let fpu_layout = Layout::from_size_align(fpu_size, 16).ok().unwrap();
        let fpu_ptr = unsafe { alloc(fpu_layout) as *mut u8 };
        let def = CLEAN_LEGACY_FPU_CXT.lock();
        let default_fpu_ref = def.as_ref().expect("Clean FPU not initialized");
        unsafe { copy_nonoverlapping(default_fpu_ref as *const LegacyXtCxt, fpu_ptr as *mut LegacyXtCxt, 1) };
        fpu_ptr as *mut u8
    };

    let stack_top = stack_base + stack_size;
    let context_addr = stack_top - size_of::<ThreadContext>();
    let context_addr = context_addr & !0xF; // align to 16 bytes
    let context = unsafe { &mut *(context_addr as *mut ThreadContext) };

    let idle_loop_addr = idle_loop as *const () as usize;

    context.zero_gp();
    context.instruction_pointer = idle_loop_addr as u64;
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
        (*tcb_ptr).priority = ThreadPriority::Idle;
    }

    tcb_ptr
}
