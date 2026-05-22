use core::hint::spin_loop;
use core::sync::atomic::Ordering;

use alloc::sync::Arc;

use crate::arch::{
    enable_interrupts,
    get_core_data,
};
use crate::drivers::keyboard::kbd_processor_thread;
use crate::kernel::object::handle::{AccessRights, HandleID};
use crate::kernel::object::invoke::{Invocation, InvocationError};
use crate::kernel::object::models::channel::init_ipc_pipeline;
use crate::kernel::object::op::{DirectoryOp, FileOp};
use crate::kernel::object::vfs::{kernel_close, kernel_invoke, kernel_walk, proc_cpy_handle};
use crate::kernel::process::pcb::ProcessControlBlock;
use crate::kernel::shell::kernel_shell_thread;
use crate::kernel::thread::dispatch::{spawn_kernel_thread, spawn_user_thread};
use crate::kernel::thread::priority::ThreadPriority;
use crate::kernel::thread::reap::reaper_daemon;
use crate::kernel::time;
use crate::kernel::time::datetime::epoch_to_datetime;
use crate::kernel::time::sleep;
use crate::memory::vmm::{VM_FLAG_EXEC, VM_FLAG_USER, VM_FLAG_WRITE};
use crate::tasks::vfs_init::init_vfs;
use crate::kernel::program::load_elf;
use crate::tests::smp_tests::{
    MUTEX_RACE,
    THREADS_FINISHED,
};
use crate::{
    KERNEL_PROCESS, klogln, terminate_thread, tests
};

// Kernel initialization tasks

// Init function dispatcher
pub extern "C" fn initializer(_arg: usize) -> ! {
    tests::memory_tests::run_pmm_tests();

    init_vfs();

    spawn_kernel_thread(reaper_daemon as *const () as usize, 0, ThreadPriority::REAPER, KERNEL_PROCESS.clone());

    let (kbd_handle, shell_handle) = init_ipc_pipeline();

    spawn_kernel_thread(kbd_processor_thread as *const () as usize, kbd_handle.0, ThreadPriority::HIGH, KERNEL_PROCESS.clone());
    spawn_kernel_thread(kernel_shell_thread as *const () as usize, shell_handle.0, ThreadPriority::MEDIUM, KERNEL_PROCESS.clone());

    let file_handle = kernel_walk("/Documents/filetest.txt", HandleID(0)).expect("File not found!");
    let mut buf = [0u8; 64];

    let read_op = FileOp::Read { offset: 0, buffer_ptr: buf.as_mut_ptr(), len: buf.len() };
    let bytes_read = kernel_invoke(file_handle, Invocation::File(read_op)).expect("Failed to read");

    klogln!("Ramdisk read success: {}", core::str::from_utf8(&buf[..bytes_read]).unwrap());

    kernel_invoke(HandleID(0), Invocation::Directory(DirectoryOp::List(0))).expect("Cannot print root directory tree");

    let path: &str = "/Programs/loop";
    spawn_kernel_thread(
        launch_user_prog as *const () as usize,
        &path as *const &str as usize,
        ThreadPriority::HIGH,
        KERNEL_PROCESS.clone()
    );
    terminate_thread!();
}

pub extern "C" fn watchdog(threads: usize) -> ! {
    loop {
        if THREADS_FINISHED.load(Ordering::Relaxed) == threads {
            let guard = MUTEX_RACE.lock();
            let counter = *guard;
            drop(guard);
            klogln!("All threads finished. Final count: {}", counter);
            break;
        } else {
            sleep(1_000_000_000);
        }
    }
    terminate_thread!();
}

pub extern "C" fn time_print_dispatcher(_arg: usize) -> ! {
    loop {
        spawn_kernel_thread(time_print as *const () as usize, 0, ThreadPriority::MEDIUM, KERNEL_PROCESS.clone());
        sleep(1_000_000_000);
    }
}

pub extern "C" fn time_print(_arg: usize) -> ! {
    enable_interrupts();
    klogln!("Current time is: {}", epoch_to_datetime(time::get_realtime()));
    terminate_thread!();
}

pub extern "C" fn test_userspace(_arg: usize) -> ! {
    loop {
        spin_loop();
    }
}

pub extern "C" fn launch_user_prog(arg: usize) -> ! {
    let path: &str = unsafe { *(arg as *const &str) };
    let file_handle = kernel_walk(path, HandleID(0)).expect("Init program binary not found!");
    let user_proc = ProcessControlBlock::new();

    let root_rights = AccessRights(AccessRights::all().0 & !AccessRights::EXECUTE.0);
    proc_cpy_handle(
        KERNEL_PROCESS.get().expect("No kernel process"),
        HandleID(0),
        &user_proc,
        root_rights,
        Some(HandleID(0))
    ).expect("Failed to copy root handle to Init");

    let entry_point = load_elf(file_handle, &user_proc).expect("Failed to load ELF");

    let stack_size = 8192;
    let stack_addr = user_proc.vmm.write()
        .mmap(stack_size, VM_FLAG_USER | VM_FLAG_WRITE)
        .expect("Failed to allocate user stack");
    let user_stack_top = stack_addr + stack_size;

    spawn_user_thread(entry_point, user_stack_top, 0, ThreadPriority::MEDIUM, user_proc);

    let _ = kernel_close(file_handle);
    terminate_thread!();
}
