use core::task::{Context, Poll};

use alloc::{boxed::Box, sync::Arc, task::Wake};
use vespertine_abi::{HandleID, Invocation};

use crate::{arch::{disable_interrupts, enable_interrupts, get_core_data, interrupts_enabled}, core::{object::{invoke::InvocationError, vfs::kernel_invoke}, thread::{ThreadControlBlock, ThreadState, dispatch::wake_thread}}};


struct ThreadWaker {
    thread: *mut ThreadControlBlock,
}

unsafe impl Send for ThreadWaker {}
unsafe impl Sync for ThreadWaker {}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        unsafe {
            if (*self.thread).state == ThreadState::Blocked {
                wake_thread(self.thread);
            }
        }
    }
}

pub fn handle_sys_invoke(handle: HandleID, invocation: Invocation) -> Result<usize, InvocationError> {
    let tcb = get_core_data().scheduler.get_current_thread();

    let mut future = Box::pin(kernel_invoke(handle, invocation));

    let waker = Arc::new(ThreadWaker { thread: tcb }).into();
    let mut context = Context::from_waker(&waker);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => {
                let int_state = interrupts_enabled();
                disable_interrupts();
                let sched = &mut get_core_data().scheduler;
                unsafe {
                    (*tcb).state = ThreadState::Blocked;
                }
                sched.schedule();
                if int_state { enable_interrupts(); } 
            }
        }
    }
}
