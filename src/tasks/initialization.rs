use core::sync::atomic::Ordering;

use crate::arch::enable_interrupts;
use crate::arch::get_core_data;
use crate::kernel::thread::dispatch::spawn_kernel_thread;
use crate::kernel::thread::priority::ThreadPriority;
use crate::kernel::thread::reap::reaper_daemon;
use crate::kernel::time;
use crate::kernel::time::datetime::epoch_to_datetime;
use crate::kernel::time::sleep;
use crate::klogln;
use crate::terminate_thread;

// Kernel initialization tasks

// Init function dispatcher
pub extern "C" fn initializer(_arg: usize) -> ! {
    spawn_kernel_thread(time_print_dispatcher as *const () as usize, 0, ThreadPriority::MEDIUM);
    spawn_kernel_thread(reaper_daemon as *const () as usize, 0, ThreadPriority::REAPER);
    terminate_thread!();
}

pub extern "C" fn time_print_dispatcher(_arg: usize) -> ! {
    loop {
        spawn_kernel_thread(time_print as *const () as usize, 0, ThreadPriority::MEDIUM);
        sleep(1_000_000_000);
    }
}

pub extern "C" fn time_print(_arg: usize) -> ! {
    enable_interrupts();
    klogln!("Current time is: {}", epoch_to_datetime(time::get_realtime()));
    terminate_thread!();
}
