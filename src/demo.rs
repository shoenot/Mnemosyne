use crate::{
    arch::x86_64::cpu::core::get_core_data,
    hcf,
    kernel::{
        sync::Mutex,
        thread::ThreadState,
    },
    klogln,
};

static SHARED_COUNTER: Mutex<usize> = Mutex::new(0);

pub fn run_demo() -> ! {
    let tt1 = test_thread_1 as *const ();
    let tt2 = test_thread_2 as *const ();

    let scheduler = &mut get_core_data().scheduler;

    scheduler.spawn(tt1 as usize, 0).unwrap();
    scheduler.spawn(tt2 as usize, 0).unwrap();

    let current_thread = scheduler.get_current_thread();

    unsafe {
        (*current_thread).state = ThreadState::Terminated;
    }

    scheduler.schedule();
    hcf();
}

fn test_thread_1() -> ! {
    loop {
        klogln!("T1: attempting to lock...");

        {
            let mut guard = SHARED_COUNTER.lock();
            klogln!("T1: lock acquired! counter is: {}", *guard);

            *guard += 1;

            klogln!("T1: Releasing lock...");
        }

        get_core_data().scheduler.schedule();
    }
}

fn test_thread_2() -> ! {
    loop {
        klogln!("T2: attempting to lock...");

        {
            let mut guard = SHARED_COUNTER.lock();
            klogln!("T2: lock acquired! counter is: {}", *guard);

            *guard += 1;

            klogln!("T2: Releasing lock...");
        }

        get_core_data().scheduler.schedule();
    }
}
