#![no_std]
#![no_main]
mod arch;
mod boot;
mod drivers;
mod helpers;
mod kernel;
mod memory;
mod panic;
mod tests;
mod demo;

extern crate alloc;

use core::sync::atomic::Ordering;

use limine::mp::MpGotoFunction;

pub use boot::*;
use crate::arch::x86_64::cpu::core::{
    get_core_data,
    init_core_data,
};
use crate::arch::x86_64::interrupts::enable_interrupts;
use crate::boot::smp::{
    BSP_CR3,
    ap_entry,
    ipi_sniper_thread,
};
use crate::drivers::logger::LOGGER;
use crate::kernel::sync::TicketLock;
use crate::kernel::time;
use crate::memory::paging::get_cr3;
use crate::memory::{
    ALLOCATOR,
    BlockSize,
    BumpAllocator,
};
use crate::panic::hcf;

pub static BOOTSTRAP_ALLOC: TicketLock<BumpAllocator> = TicketLock::new(BumpAllocator::new());

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    LOGGER.lock().init();
    memory::init();

    let bootstrap_page = ALLOCATOR.lock().alloc(BlockSize::Huge).unwrap() as usize;
    BOOTSTRAP_ALLOC.lock().init(bootstrap_page);

    arch::init();
    arch::init_fpu(true);
    arch::init_bootstrap_core();
    time::init();

    let mp_resp = MP_REQUEST.response().expect("No SMP Response from limine");

    let bsp_id = mp_resp.bsp_lapic_id;

    let cr3 = get_cr3();
    BSP_CR3.store(cr3, Ordering::Relaxed);
    klogln!("{}", bsp_id);

    for core in mp_resp.cpus() {
        if core.lapic_id == bsp_id {
            continue;
        }

        unsafe {
            let ap_data_ptr = init_core_data(core.lapic_id as usize, get_core_data().apic_mode.clone());

            if core.lapic_id == 1 {
                let att = ipi_sniper_thread as *const ();
                (*ap_data_ptr).scheduler.spawn(att as usize, 0).unwrap();
            }

            let ap_data_addr = ap_data_ptr as u64;
            let ap_entry_ptr = ap_entry as MpGotoFunction;

            core.bootstrap(ap_entry_ptr, ap_data_addr);
        }
    }

    enable_interrupts();
    get_core_data().scheduler.terminate();

    unreachable!()
}
