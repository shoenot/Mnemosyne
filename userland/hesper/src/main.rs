#![no_std]
#![no_main]

use core::ptr::null;

use vespertine_abi::{AccessRights, FileOp, HandleID, Invocation, ProcManOp, ProcessInitPackage, tag::{TAG_SYS_PROCMAN, find_tag}};
use vespertine_rt::syscall::{sys_invoke, sys_lookup};

#[unsafe(no_mangle)]
pub extern "sysv64" fn main(pkg_ptr: *const ProcessInitPackage) {
    let pkg = unsafe { &*pkg_ptr };
    // userspace shell proc
    let pm_grant = match find_tag(pkg.ext(), TAG_SYS_PROCMAN) {
        Some(g) => g,
        None => panic!("Hesper reqires the ProcessManager handle to be injected"),
    };

    let pm_handle = pm_grant.id; 
    let programs_dir_handle = sys_lookup(pkg.root_handle, "Programs").expect("No programs dir");
    let shell_exec_handle = sys_lookup(programs_dir_handle, "shell").expect("No shell executable");

    let msg = "Hello from Hesper init system";
    let _ = sys_invoke(
        pkg.sink_handle, 
        &Invocation::File(FileOp::Write { offset: 0, buffer_ptr: msg.as_ptr() as *mut u8, len: msg.len() })
    );

    let shell_spawn_op = ProcManOp::Spawn { 
        exec_handle: shell_exec_handle, 
        root_handle: pkg.root_handle, 
        root_rights: AccessRights::all(), 
        source: pkg.source_handle,
        sink: pkg.sink_handle,
        extra_handles_ptr: null(),
        extra_handles_len: 0,
        args_buffer_ptr: null(),
        args_buffer_len: 0,
    };

    sys_invoke(pm_handle, &Invocation::ProcessManager(shell_spawn_op))
        .expect("Failed to spawn shell from hesper");
}
