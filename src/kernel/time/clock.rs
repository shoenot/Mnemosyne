use core::arch::asm;
use core::mem::transmute;
use core::sync::atomic::Ordering;

use crate::arch::x86_64::apic::lapic::ApicDriver;
use crate::arch::x86_64::cpu::core::get_core_data;
use crate::arch::x86_64::interrupts::{
    disable_interrupts,
    enable_interrupts,
};
use crate::kernel::thread::ThreadState;
use crate::kernel::time::{
    GET_TIME_FN,
    IA32_TSC_DEADLINE,
    LAPIC_FQ,
    TIME_SRC_FQ,
    TimeFn,
    USE_TSC_DEADLINE,
};

pub fn arm_sleep_ns(ns: usize) {
    if USE_TSC_DEADLINE.load(Ordering::Relaxed) {
        let tsc_fq = TIME_SRC_FQ.load(Ordering::Relaxed);
        let tsc_ticks = (ns * tsc_fq) / 1_000_000_000;

        let mut lo: u32;
        let mut hi: u32;
        unsafe {
            // read tsc
            asm!("rdtsc",
                out("eax") lo, out("edx") hi, options(nomem, nostack));

            let current = ((hi as usize) << 32) | (lo as usize);
            let target = current + tsc_ticks;
            let tgt_lo = (target & 0xFFFF_FFFF) as u32;
            let tgt_hi = (target >> 32) as u32;

            // set deadline
            asm!("wrmsr",
                in("ecx") IA32_TSC_DEADLINE, in("eax") tgt_lo, in("edx") tgt_hi, options(nomem, nostack));
        }
    } else {
        let lapic_fq = LAPIC_FQ.load(Ordering::Relaxed);
        let lapic_ticks = (ns as usize * lapic_fq) / 1_000_000_000;

        let core_data = get_core_data();
        core_data.apic_mode.arm_oneshot(lapic_ticks as u32);
    }
}

pub fn arm_sleep_ticks(ticks: usize) {
    if USE_TSC_DEADLINE.load(Ordering::Relaxed) {
        let mut lo: u32;
        let mut hi: u32;
        unsafe {
            // read tsc
            asm!("rdtsc",
                out("eax") lo, out("edx") hi, options(nomem, nostack));

            let current = ((hi as usize) << 32) | (lo as usize);
            let target = current + ticks;
            let tgt_lo = (target & 0xFFFF_FFFF) as u32;
            let tgt_hi = (target >> 32) as u32;

            // set deadline
            asm!("wrmsr",
                in("ecx") IA32_TSC_DEADLINE, in("eax") tgt_lo, in("edx") tgt_hi, options(nomem, nostack));
        }
    } else {
        let global_fq = TIME_SRC_FQ.load(Ordering::Relaxed) as u128;
        let lapic_fq = LAPIC_FQ.load(Ordering::Relaxed) as u128;

        let lapic_ticks = (ticks as u128 * lapic_fq) / global_fq;
        let core_data = get_core_data();
        core_data.apic_mode.arm_oneshot(lapic_ticks as u32);
    }
}

pub fn ns_to_ticks(ns: usize) -> usize {
    let freq = TIME_SRC_FQ.load(Ordering::Relaxed);
    ((ns as u128 * freq as u128) / 1_000_000_000) as usize
}

pub fn get_time() -> usize {
    let ptr = GET_TIME_FN.load(Ordering::Relaxed);
    let time_func: TimeFn = unsafe { transmute(ptr) };
    time_func()
}

pub fn sleep(ns: usize) {
    let target_time = get_time() + ns_to_ticks(ns);

    disable_interrupts();

    let core_data = get_core_data();
    let sched = &mut core_data.scheduler;

    let current_thread = sched.get_current_thread();

    unsafe {
        (*current_thread).state = ThreadState::Blocked;
        (*current_thread).wake_time = target_time;
    }

    sched.push_sleep(current_thread);

    if sched.sleep_queue_head == current_thread {
        arm_sleep_ns(ns);
    }

    sched.schedule();

    enable_interrupts();
}
