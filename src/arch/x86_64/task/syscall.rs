use crate::{arch::x86_64::task::context::SyscallFrame, kernel::object::{invoke::Invocation, vfs::sys_invoke}};

pub extern "C" fn syscall_dispatch(frame: *mut SyscallFrame) {
    unsafe {
        let syscall_number = (*frame).rdi;
        let handle_id = (*frame).rdi;
        let invocation_ptr = (*frame).rsi as *const Invocation;

        let result = match syscall_number {
            0 => sys_invoke(handle_id, invocation_ptr)
        }
    }
}
