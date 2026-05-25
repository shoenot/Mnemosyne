#![no_std]
#![no_main]

extern crate alloc;

use alloc::str;
use vespertine_abi::DirectoryOp;
use vespertine_abi::FileOp;
use vespertine_abi::HandleID;
use vespertine_abi::Invocation;
use vespertine_abi::ProcessInitPackage;
use vespertine_abi::tag::TAG_SYS_PROCMAN;
use vespertine_abi::tag::TAG_SYS_SOCKFAC;
use vespertine_abi::tag::find_tag;
use vespertine_rt::print;
use vespertine_rt::println;
use vespertine_rt::source::read_line;
use vespertine_rt::syscall::SysError;
use vespertine_rt::syscall::sys_close;
use vespertine_rt::syscall::sys_create_socket;
use vespertine_rt::syscall::sys_invoke;
use vespertine_rt::syscall::walk_path;

#[unsafe(no_mangle)]
pub extern "sysv64" fn main(pkg_ptr: *const ProcessInitPackage) {
    let pkg = unsafe { &*pkg_ptr };
    let pm = find_tag(pkg.ext(), TAG_SYS_PROCMAN).map(|g| g.id);
    let sf = find_tag(pkg.ext(), TAG_SYS_SOCKFAC).map(|g| g.id);

    loop {
        print!(">> ");
        let mut buf = [0u8; 128];
        let n = read_line(&mut buf);
        let line = str::from_utf8(&buf[..n])
            .unwrap_or("")
            .trim_end_matches('\n')
            .trim();

        let mut parts = line.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        match cmd {
            "" => {},
            "ls" => {
                let (read_end, write_end) = sys_create_socket(sf.unwrap())
                    .expect("Shell could not invoke SocketFactory");

                let op = Invocation::Directory(
                    DirectoryOp::List { offset: 0, sink: write_end }
                );
                let _ = sys_invoke(pkg.root_handle, &op);
                let _ = sys_close(write_end);
                pipe_to_sink(read_end, pkg.sink_handle);
                let _ = sys_close(read_end);
            },
            "cat" => cmd_cat(arg, pkg),
            "echo" => cmd_echo(arg),
            other => {println!("unknown command: {}", other)},
        }
    }
}

fn cmd_echo(text: &str) {
    println!("{}", text);
}

fn cmd_cat(path: &str, pkg: &ProcessInitPackage) {
    let handle = match walk_path(path, pkg.root_handle) {
        Ok(h) => h,
        Err(_) => { println!("cat: no such file: {}", path); return; }
    };

    let mut buf = [0u8; 256];
    let mut offset = 0;
    loop {
        let op = Invocation::File(
            FileOp::Read { offset, buffer_ptr: buf.as_mut_ptr(), len: buf.len() }
        );
        match sys_invoke(handle, &op) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let op = Invocation::File(
                    FileOp::Write { offset: 0, buffer_ptr: buf.as_mut_ptr(), len: buf.len() }
                );
                let _ = sys_invoke(pkg.sink_handle, &op);
                offset += n;
            },
        }
    }
}

pub fn pipe_to_sink(source: HandleID, sink: HandleID) {
    let mut buf = [0u8; 128];
    loop {
        let op = Invocation::File(
            FileOp::Read { offset: 0, buffer_ptr: buf.as_mut_ptr(), len: buf.len() }
        );
        match sys_invoke(source, &op) {
            Ok(0) | Err(_) => break,        // EOF or Error
            Ok(n) => {
                let op = Invocation::File(
                    FileOp::Write { offset: 0, buffer_ptr: buf.as_mut_ptr(), len: n }
                );
                let _ = sys_invoke(sink, &op);
            }
        }
    }
}

