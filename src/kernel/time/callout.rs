use core::{ptr::null_mut, sync::atomic::AtomicPtr};

use alloc::collections::binary_heap::BinaryHeap;

use crate::{arch::{disable_interrupts, enable_interrupts, get_core_data, x86_64::{apic::lapic::ApicDriver, cpu::core::CPULocalData}}, kernel::{cpu::get_core_data_for, sync::{KernelOnceCell, TicketLock}, thread::{ThreadControlBlock, ThreadState, schedule::DEFAULT_QUANTUM}, time::{arm_sleep_ticks, get_time}}};

pub enum CalloutPayload {
    /// Used by 'sleep()'. Contains the pointer to the sleeping thread.
    WakeThread(*mut ThreadControlBlock),
}

pub struct Callout {
    pub wake_time: usize,
    pub payload: CalloutPayload,
}

// Flip the cmp logic backwards bc we want the earliest callout to rise to the top

impl PartialEq for Callout {
    fn eq(&self, other: &Self) -> bool {
        self.wake_time == other.wake_time
    }
}

impl Eq for Callout {}

impl Ord for Callout {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        other.wake_time.cmp(&self.wake_time)
    }
}

impl PartialOrd for Callout {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

unsafe impl Send for Callout {}

pub extern "C" fn timer_daemon(_arg: usize) -> ! {
    loop {
        let current_time = get_time();
        let mut queue = get_core_data().callout_queue.lock();

        if let Some(earliest) = queue.peek() {
            if earliest.wake_time <= current_time {
                // expired, pop it off
                let expired = queue.pop().unwrap();
                drop(queue);

                match expired.payload {
                    CalloutPayload::WakeThread(tcb_ptr) => {
                        unsafe {
                            (*tcb_ptr).state = ThreadState::Ready;
                            get_core_data().scheduler.push(tcb_ptr);
                        }
                    },
                }

                continue;
            }
            
            let next_wake = earliest.wake_time;
            drop(queue);

            disable_interrupts();
            unsafe { (*get_core_data().scheduler.current_thread).state = ThreadState::Blocked };
            
            arm_sleep_ticks(next_wake.saturating_sub(get_time())); 
            
            get_core_data().scheduler.schedule();
            enable_interrupts();
            
        } else {
            drop(queue);
            
            disable_interrupts();
            unsafe { (*get_core_data().scheduler.current_thread).state = ThreadState::Blocked };
            
            get_core_data().scheduler.schedule();
            
            enable_interrupts();
        }
    }
}
