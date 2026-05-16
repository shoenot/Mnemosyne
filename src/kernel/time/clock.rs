use core::arch::asm;
use core::mem::transmute;
use core::sync::atomic::Ordering;

use crate::arch::get_rtc_unix_timestamp;
use crate::arch::x86_64::apic::lapic::ApicDriver;
use crate::arch::x86_64::cpu::core::get_core_data;
use crate::arch::x86_64::interrupts::{
    disable_interrupts,
    enable_interrupts,
};
use crate::kernel::sync::KernelOnceCell;
use crate::kernel::thread::ThreadState;
use crate::kernel::time::callout::{Callout, CalloutPayload};
use crate::kernel::time::{
    GET_TIME_FN,
    IA32_TSC_DEADLINE,
    LAPIC_FQ,
    TIME_SRC_FQ,
    TimeFn,
    USE_TSC_DEADLINE,
};

static BOOT_RTC_TIMESTAMP: KernelOnceCell<i64> = KernelOnceCell::new();
static BOOT_TIMESTAMP: KernelOnceCell<i64> = KernelOnceCell::new();

pub fn init_realtime() {
    BOOT_RTC_TIMESTAMP.get_or_init(|| get_rtc_unix_timestamp());
    BOOT_TIMESTAMP.get_or_init(|| get_time() as i64);
}

pub fn get_realtime() -> i64 {
    let seconds_passed = (get_time() as i64 - *BOOT_TIMESTAMP) / *TIME_SRC_FQ as i64;
    *BOOT_RTC_TIMESTAMP + seconds_passed
}

pub fn arm_sleep_ns(ns: usize) {
    if USE_TSC_DEADLINE.load(Ordering::Relaxed) {
        let tsc_fq = *TIME_SRC_FQ;
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
        let lapic_fq = *LAPIC_FQ;
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
        let global_fq = *TIME_SRC_FQ;
        let lapic_fq = *LAPIC_FQ;

        let lapic_ticks = (ticks as u128 * lapic_fq as u128) / global_fq as u128;
        let core_data = get_core_data();
        core_data.apic_mode.arm_oneshot(lapic_ticks as u32);
    }
}

pub fn ns_to_ticks(ns: usize) -> usize { ((ns as u128 * *TIME_SRC_FQ as u128) / 1_000_000_000) as usize }

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

    let callout = Callout {
        wake_time: target_time,
        payload: CalloutPayload::WakeThread(current_thread),
    };

    let mut queue = get_core_data().callout_queue.lock();
    queue.push(callout);
    
    let is_earliest = queue.peek().unwrap().wake_time == target_time;
    drop(queue);

    if is_earliest {
        let daemon_ptr = get_core_data().timer_daemon_tcb;
        if !daemon_ptr.is_null() {
            unsafe {
                if (*daemon_ptr).state == ThreadState::Blocked {
                    (*daemon_ptr).state = ThreadState::Ready;
                    sched.push(daemon_ptr);
                }
            }
        }
    }

    sched.schedule();

    enable_interrupts();
}
