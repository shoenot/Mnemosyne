use core::{ptr::write_volatile, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};

use crate::{kernel::{object::{handle::{AccessRights, HandleID, HandleTable}, invoke::{Invocation, InvocationError}, obj::{HandleEntry, KernelObject}, op::ProcOp, vfs::{ROOT_DIRECTORY, kernel_duplicate, proc_cpy_handle, proc_register_obj}}, sync::RwLock, thread::{get_current_process, dispatch::spawn_user_thread, priority::ThreadPriority}, program::load_elf}, memory::{ALLOCATOR, vmm::{VirtMemManager, VM_FLAG_USER, VM_FLAG_WRITE}}};
use alloc::sync::Arc;

pub static GLOBAL_PID: AtomicUsize = AtomicUsize::new(0);

pub fn get_new_pid() -> usize {
    GLOBAL_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

pub type Process = Arc<ProcessControlBlock>;

pub struct ProcStatus {
    pub pid: usize,
    pub active_threads: usize,
    pub is_terminated: bool,
    pub memory_usage: usize,
}

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
    pub fn new() -> Process {
        Arc::new(
            Self {
                proc_id: get_new_pid(),
                proc_handles: RwLock::new(HandleTable::new()),
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
        unsafe { write_volatile(ptr, proc_status) };
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
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}
