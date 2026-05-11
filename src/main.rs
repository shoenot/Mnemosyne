#![allow(unreachable_code)]
#![no_std]
#![no_main]
mod arch;
mod boot;
mod drivers;
mod kernel;
mod panic;
mod tests;

extern crate alloc;
use core::arch::asm;

pub use arch::x86_64::{
    IO_APIC,
    LOCAL_APIC,
};
use arch::x86_64::{
    init_apic,
    init_interrupts,
};
pub use boot::*;
use kernel::{
    lock::TicketLock,
    memory::{
        heap::KernelAllocator,
        paging::*,
        pmm::*,
        vmm::*,
    },
    thread::schedule::*,
    time,
    time::*,
};
use panic::hcf;
use tests::memory_tests::*;

use crate::arch::x86_64::io::{inb, outb};

#[global_allocator]
pub static KERNEL_ALLOCATOR: KernelAllocator = KernelAllocator::new();

static ALLOCATOR: TicketLock<Allocator> = TicketLock::new(Allocator::new());
static PAGER: TicketLock<Pager> = TicketLock::new(Pager::new(&ALLOCATOR));
static GLOBAL_VMM: TicketLock<VirtMemManager> = TicketLock::new(VirtMemManager::new(&PAGER, &ALLOCATOR));

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    if !BASE_REVISION.is_supported() {
        hcf();
    }

    init_interrupts();

    klogln!("INITIATING MEMORY MANAGERS... ");

    // Inititate PMM
    {
        let mut allocator = ALLOCATOR.lock();
        allocator.init();
    }

    // Inititate Pager
    {
        let mut pager = PAGER.lock();
        pager.init();
    }

    klogln!("SWITCHED CR3. PAGING HANDOVER COMPLETE.");

    klogln!("RUNNING MEMORY TESTS");

    test_kmalloc();
    test_vmalloc();
    test_collections();

    klogln!("TESTS COMPLETE!");

    init_apic();

    unsafe {
        let mut cr4: usize;
        asm!("mov {}, cr4",
            out(reg) cr4);
        cr4 |= 1 << 9; // set bit 9 
        asm!("mov cr4, {}",
            in(reg) cr4);
    }

    time::init();
    klogln!("Using timer: {:#?} with frequency: {:?}", *TIME_SOURCE.lock(), TIME_SRC_FQ);
    klogln!("");

    init_clean_fpu();

    let tt1 = test_thread_1 as *const ();
    let tt2 = test_thread_2 as *const ();

    SCHEDULER.lock().spawn(tt1 as usize).unwrap();
    SCHEDULER.lock().spawn(tt2 as usize).unwrap();

    arm_sleep_ns(10_000_000);

    SCHEDULER.lock().schedule();

    hcf();
}

fn test_thread_1() -> ! {
    loop {
        klogln!("A");
    }
}

fn test_thread_2() -> ! {
    loop {
        klogln!("B");
    }
}
