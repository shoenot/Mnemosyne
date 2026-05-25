#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::ToString;
use vespertine_abi::FileOp;
use vespertine_abi::Invocation;
use vespertine_abi::ProcessInitPackage;
use vespertine_rt::syscall::sys_invoke;

fn console_write(text: &str) -> Invocation {
    Invocation::File(FileOp::Write { 
        offset: 0, 
        buffer_ptr: text.as_ptr() as *mut u8,
        len: text.len() 
    })
}

#[unsafe(no_mangle)]
pub extern "sysv64" fn main(pkg_ptr: *const ProcessInitPackage) {
    let pkg = unsafe { &*pkg_ptr };

    let text = "Hello from userland shell program\n".to_string();
    let op = console_write(&text);

    let _ = sys_invoke(pkg.sink_handle, &op);
}
