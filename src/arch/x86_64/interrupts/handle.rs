use core::arch::asm;

use crate::arch::x86_64::apic::lapic::{
    ApicDriver,
};
use crate::arch::x86_64::cpu::core::get_core_data;
use crate::arch::x86_64::interrupts::idt::InterruptStackFrame;
use crate::kernel::thread::tcb::ThreadState;
use crate::klogln;
use crate::memory::GLOBAL_VMM;

pub(in crate::arch::x86_64::interrupts) fn page_fault_handler(frame: &mut InterruptStackFrame) {
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
    }

    let mut vmm = GLOBAL_VMM.lock();
    if !vmm.handle_page_fault(cr2 as usize, frame.error_code as usize) {
        panic!("FATAL: Unhandled Page Fault!");
    }
}

pub(in crate::arch::x86_64::interrupts) fn gpf_handler(frame: &mut InterruptStackFrame) {
    klogln!("General Protection Fault.\nError Code: {:#X}\nStack Frame:\n{:#?}", frame.error_code, frame);
    crate::hcf();
}

pub(in crate::arch::x86_64::interrupts) fn unexpected_interrupt_handler(frame: &mut InterruptStackFrame) {
    klogln!("Unexpected Interrupt.\nStack Frame:\n{:#?}", frame);
}

pub(in crate::arch::x86_64::interrupts) fn timer_interrupt_handler() {
    let core_data = get_core_data();
    klogln!("timer interrupt");

    core_data.apic_mode.eoi(); 

    unsafe {
        let td_tcb_ptr = (*core_data).timer_daemon_tcb;
        if (*td_tcb_ptr).state == ThreadState::Blocked {
            (*td_tcb_ptr).state = ThreadState::Ready;
            core_data.scheduler.push(td_tcb_ptr);
        }
    }

    core_data.scheduler.schedule();
}

pub(in crate::arch::x86_64::interrupts) fn ipi_handler() {
    let core_data = get_core_data();
    core_data.apic_mode.eoi();
    klogln!(">>> Core {} forcefully woken up by an IPI <<<", core_data.lapic_id);
    core_data.scheduler.schedule();
}
