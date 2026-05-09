use super::idt::InterruptStackFrame;
use crate::drivers::serial::SerialWriter;

use core::fmt::Write;


pub fn gpf_handler(frame: &InterruptStackFrame) {
    log_to_serial("Error Code: ");
    log_u64_to_serial(frame.error_code);
    log_to_serial(" Instruction Pointer: ");
    log_u64_to_serial(frame.instruction_pointer);
    hcf();
}

pub fn read_cr2() -> u64 {
    let cr2: u64;
    unsafe {
        asm!("movq %cr2, {0}", out(reg) cr2, options(att_syntax, nostack, preserves_flags));
    };
    cr2
}

pub fn page_fault_handler(frame: &InterruptStackFrame) {
    let addr = read_cr2() as usize;
    let error_code = frame.error_code as usize;
    let mut vmm = GLOBAL_VMM.lock();

    let fixed = vmm.handle_page_fault(addr, error_code);

    if !fixed {
        panic!(
            "PAGE FAULT EXCEPTION\nAT ADDRESS: {:#X}\nError Code: {:#b}\n{:#?}",
            addr, error_code, frame
        )
    }
}

pub fn timer_handler(frame: &InterruptStackFrame) {
    unsafe {
        let lapic = Local_APIC { base_addr: get_apic_base() + *HHDMOFFSET };
        lapic.eoi();
    }
}
