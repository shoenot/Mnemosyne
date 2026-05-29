use core::{ptr::addr_of, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};

use crate::{
    arch::{
        disable_interrupts, 
        enable_interrupts,
        get_core_data,
        interrupts_enabled,
        x86_64::task::syscall::{safe_copy_from, safe_copy_to},
    }, 
    core::{
        object::{
            handle::HandleTable, 
            invoke::InvocationError, 
            models::{socket::SocketEndpoint, thread::Thread}, 
            obj::KernelObject, 
        },
        sync::RwLock, 
        thread::{ThreadState, dispatch::spawn_user_thread, get_current_process, priority::ThreadPriority, wait::WakeToken}
    }, 
    memory::{ALLOCATOR, vmm::VirtMemManager}
};
use vespertine_abi::{Invocation, Signal, WaitItem, WaitOp};
use alloc::{sync::Arc, vec::Vec};

use vespertine_abi::op::ProcOp;
use vespertine_abi::ProcStatus;
use vespertine_abi::{AccessRights, HandleID};
use alloc::vec;

pub static GLOBAL_PID: AtomicUsize = AtomicUsize::new(0);

pub fn get_new_pid() -> usize {
    GLOBAL_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

pub type Process = Arc<ProcessControlBlock>;

#[repr(C)]
#[derive(Debug)]
pub struct ProcessControlBlock {
    pub proc_id: usize,
    pub proc_handles: RwLock<HandleTable>,
    pub vmm: RwLock<VirtMemManager>,
    pub active_threads: AtomicUsize,
    pub is_terminated: AtomicBool,
}

impl ProcessControlBlock {
    pub fn new(init_table: HandleTable) -> Process {
        Arc::new(
            Self {
                proc_id: get_new_pid(),
                proc_handles: RwLock::new(init_table),
                vmm: RwLock::new(VirtMemManager::new(&ALLOCATOR)),
                active_threads: AtomicUsize::new(0),
                is_terminated: AtomicBool::new(false),
            }
        )
    }

    pub fn status(&self, ptr: *mut ProcStatus) -> Result<usize, InvocationError> {
        let proc_status = ProcStatus { 
            pid: self.proc_id,
            active_threads: self.active_threads.load(Ordering::Relaxed),
            is_terminated: self.is_terminated.load(Ordering::Relaxed),
            memory_usage: self.vmm.read().get_total_allocated_size(),
        };
        let src_ptr = addr_of!(proc_status) as *const u8;
        safe_copy_to(ptr as *mut u8, src_ptr, size_of::<ProcStatus>());
        Ok(0)
    }
}

impl KernelObject for ProcessControlBlock {
    fn type_name(&self) -> &'static str {
        "Process"
    }

    fn invoke(&self, invocation: Invocation, _calling_rights: AccessRights) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::Proc(ProcOp::Kill) => { self.is_terminated.store(true, Ordering::SeqCst); Ok(0) },
            Invocation::Proc(ProcOp::GetStatus { status_ptr }) => self.status(status_ptr),
            Invocation::Proc(ProcOp::Unmap { vaddr, len } ) => {
                self.vmm.write().munmap(vaddr, len).map(|_| 0).map_err(|_| InvocationError::InvalidArgument)
            },
            Invocation::Proc(ProcOp::SpawnThread { entry, stack_top, arg, priority }) => {
                let tp = ThreadPriority::from(priority);
                let proc = get_current_process().ok_or(InvocationError::ThreadSpawnFail)?;
                let thread = spawn_user_thread(entry, stack_top, arg, tp, proc.clone());
                self.active_threads.fetch_add(1, Ordering::Relaxed);
                let obj = Arc::new(Thread { tcb: thread });
                let id = self.proc_handles.write().insert(obj, AccessRights::all());
                Ok(id.0)
            },
            Invocation::Wait(WaitOp::Many { items_ptr, count }) => {
                if count == 0 || count > 64 {
                    return Err(InvocationError::InvalidArgument);
                }

                let mut items = vec![WaitItem { 
                    handle: HandleID(0),
                    signal: Signal(0),
                    pending: Signal(0),
                }; count];

                if !safe_copy_from(items.as_mut_ptr() as *mut u8, items_ptr as *const u8, count * size_of::<WaitItem>()) {
                    return Err(InvocationError::InvalidPointer);
                }

                let mut endpoints: Vec<Arc<SocketEndpoint>> = Vec::with_capacity(count);

                {
                    let table = self.proc_handles.read();
                    for item in &items {
                        let ep = {
                            let entry = table.resolve_entry(item.handle, AccessRights::READ)?;
                            if entry.object.type_name() != "Socket" {
                                return Err(InvocationError::UnsupportedOperation);
                            }
                            unsafe {
                                Arc::from_raw(Arc::into_raw(entry.object.clone()) as *const SocketEndpoint)
                            }
                        };
                        endpoints.push(ep);
                    }
                }

                loop {
                    // poll each ep for satisfied signals 
                    let mut any_ready = false;
                    for (i, ep) in endpoints.iter().enumerate() {
                        items[i].pending = Signal(0);
                        let sig = items[i].signal;

                        if sig.contains(Signal::READABLE) {
                            let bus = ep.read_bus.buffer.lock();
                            if !bus.is_empty() || ep.read_bus.is_closed.load(Ordering::SeqCst) {
                                items[i].pending = items[i].pending | Signal::READABLE;
                                any_ready = true;
                            }
                        }

                        if sig.contains(Signal::PEER_CLOSED) {
                            if ep.read_bus.is_closed.load(Ordering::SeqCst) {
                                items[i].pending = items[i].pending | Signal::PEER_CLOSED;
                                any_ready = true;
                            }
                        }
                    }

                    if any_ready {
                        safe_copy_to(items_ptr as *mut u8, items.as_ptr() as *const u8, count * size_of::<WaitItem>());
                        return Ok(0);
                    }

                    let int_state = interrupts_enabled();
                    disable_interrupts();

                    let sched = &mut get_core_data().scheduler;
                    let thread = sched.current_thread;

                    let mut token = WakeToken::new(thread);
                    let token_ptr = &mut token as *mut WakeToken;

                    for ep in &endpoints {
                        ep.read_bus.multi_read_waiters.lock().push(token_ptr);
                    }

                    unsafe { (*thread).state = ThreadState::Blocked; }
                    sched.schedule();

                    for ep in &endpoints {
                        ep.read_bus.multi_read_waiters.lock().remove(token_ptr);
                    }

                    if int_state { enable_interrupts(); }
                
                }
            },
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}
