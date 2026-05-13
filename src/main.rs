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

use core::{hint::spin_loop, sync::atomic::Ordering};

pub use boot::*;
use limine::mp::MpGotoFunction;

use crate::{
    arch::x86_64::{
        cpu::core::{
            get_core_data,
            init_core_data,
        },
        interrupts::enable_interrupts,
    }, boot::{
        MP_REQUEST,
        smp::{
            BSP_CR3, THREADS_FINISHED, ap_entry, ap_test_thread, contention_thread
        },
    }, demo::run_demo, drivers::logger::LOGGER, kernel::{sync::TicketLock, thread::schedule::DEFAULT_QUANTUM, time::{self, arm_sleep_ns}}, memory::{ALLOCATOR, BlockSize, BumpAllocator, paging::get_cr3}, panic::hcf, smp::RACE_COUNTER
};

pub static BOOTSTRAP_ALLOC: TicketLock<BumpAllocator> = TicketLock::new(BumpAllocator::new());

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    LOGGER.lock().init();
    memory::init();

    let bootstrap_page = ALLOCATOR.lock().alloc(BlockSize::Huge).unwrap() as usize;
    BOOTSTRAP_ALLOC.lock().init(bootstrap_page);

    arch::init();
    arch::init_fpu();
    arch::init_bootstrap_core();
    time::init();

    let mp_resp = MP_REQUEST.response().expect("No SMP Response from limine");

    let bsp_id = mp_resp.bsp_lapic_id;

    let cr3 = get_cr3();
    BSP_CR3.store(cr3, Ordering::Relaxed);

    for core in mp_resp.cpus() {
        if core.lapic_id == bsp_id {
            continue;
        }

        unsafe {
            let ap_data_ptr = init_core_data(core.lapic_id as usize, get_core_data().apic_mode.clone());

            let att = contention_thread as *const ();
            for i in 0..4 {
                (*ap_data_ptr).scheduler.spawn(att as usize, i).unwrap();
            }

            let ap_data_addr = ap_data_ptr as u64;
            let ap_entry_ptr = ap_entry as MpGotoFunction;

            core.bootstrap(ap_entry_ptr, ap_data_addr);
        }
    }

    arm_sleep_ns(DEFAULT_QUANTUM);

    while THREADS_FINISHED.load(Ordering::Relaxed) < 12 {
        spin_loop();
    }

    {
        klogln!("final counter: {}", *RACE_COUNTER.lock());
    }

    get_core_data().scheduler.schedule();
    hcf();
}
