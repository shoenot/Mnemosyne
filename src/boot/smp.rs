use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use limine::mp::MpInfo;

use crate::{
    arch::{
        init_ap_fpu,
        init_fpu,
        x86_64::{
            apic::lapic::{
                ApicDriver,
                ApicMode,
                TimerMode,
                init_local_apic,
            },
            cpu::core::{
                CPULocalData,
                activate_core,
                get_core_data,
            },
            interrupts::{
                enable_interrupts,
                idt::load_idt,
            },
        },
    },
    hcf,
    kernel::{
        sync::TicketLock, thread::{ThreadState, schedule::DEFAULT_QUANTUM}, time::{
            USE_TSC_DEADLINE, arm_sleep_ns, sleep
        }
    },
    klogln, memory::paging::load_cr3,
};

pub static BSP_CR3: AtomicU64 = AtomicU64::new(0);

pub extern "C" fn ap_entry(mp_info: &MpInfo) -> ! {
    load_cr3(BSP_CR3.load(Ordering::Relaxed));
    let core_data_ptr = mp_info.extra_argument() as *mut CPULocalData;
    activate_core(core_data_ptr);

    load_idt();
    init_ap_fpu();

    let core_data = get_core_data();

    match &mut core_data.apic_mode {
        ApicMode::XApic(a) => {
            a.init();
        }
        ApicMode::X2Apic(a) => {
            a.init();
        }
    }

    if USE_TSC_DEADLINE.load(Ordering::Relaxed) {
        core_data.apic_mode.timer_setup(35, 0, TimerMode::TscDeadline);
    } else {
        core_data.apic_mode.timer_setup(35, 0, TimerMode::OneShot);
    }

    klogln!("Started {}", get_core_data().lapic_id);

    enable_interrupts();

    let scheduler = &mut core_data.scheduler;
    let current_thread = scheduler.get_current_thread();
    unsafe { (*current_thread).state = ThreadState::Terminated; }
    arm_sleep_ns(DEFAULT_QUANTUM);
    scheduler.schedule();
    hcf();
}

#[allow(dead_code)]
pub fn ap_test_thread(thread_id: usize) -> ! {
    let mut count: usize = 0;
    loop {
        klogln!("This is thread {} on core {} and the counter is at {}", thread_id, get_core_data().lapic_id, count);
        count += 1;
    }
}

pub static RACE_COUNTER: TicketLock<usize> = TicketLock::new(0);
pub static THREADS_FINISHED: AtomicUsize = AtomicUsize::new(0);

pub extern "C" fn contention_thread(_id: usize) -> ! {
    for _ in 0..10_000_000 {
        let mut guard = RACE_COUNTER.lock();
        let val = *guard;
        *guard = val + 1;
    }

    THREADS_FINISHED.fetch_add(1, Ordering::Relaxed);
    
    loop {
        crate::kernel::time::sleep(1_000_000); 
    }
}
