use core::{alloc::Layout, hint, sync::atomic::{AtomicUsize, Ordering}};

use alloc::{alloc::dealloc, vec::Vec};

use crate::{arch::{get_core_data, x86_64::apic::lapic::ApicDriver}, kernel::{cpu::{NUM_CORES, get_core_data_for}, sync::TicketLock}, memory::{GLOBAL_VMM, paging::flush_tlb}};

pub struct TLBShootdownInfo {
    pub addr: AtomicUsize,
    pub counter: AtomicUsize,
}

pub static SHOOTDOWN_INFO: TLBShootdownInfo = TLBShootdownInfo { addr: AtomicUsize::new(0), counter: AtomicUsize::new(0) };
pub static SHOOTDOWN_LOCK: TicketLock<()> = TicketLock::new(());

pub fn shootdown(addr: usize, size: usize) {
    let this_core_id = get_core_data().logical_id;
    let lock = SHOOTDOWN_LOCK.lock();
    SHOOTDOWN_INFO.addr.store(addr, Ordering::Release);
    SHOOTDOWN_INFO.counter.store(*NUM_CORES - 1, Ordering::Release);
    for id in 0..*NUM_CORES {
        if id == this_core_id {
            continue
        }
        get_core_data_for(id).apic_mode.send_ipi(get_core_data_for(id).lapic_id as u32, 65);
    }
    flush_tlb(addr as u64);
    while SHOOTDOWN_INFO.counter.load(Ordering::Acquire) != 0 {
        hint::spin_loop();
    }
    drop(lock);
}

