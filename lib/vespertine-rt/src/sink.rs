use core::fmt;

use vespertine_abi::{FileOp, HandleID, Invocation};

use crate::syscall::sys_invoke;

pub struct SinkWriter;

impl fmt::Write for SinkWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let op = Invocation::File(
            FileOp::Write { 
                offset: 0, 
                buffer_ptr: s.as_ptr() as *mut u8, 
                len: s.len() 
            }
        );
        let _ = sys_invoke(HandleID(3), &op);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        { let _ = core::fmt::write(
            &mut $crate::sink::SinkWriter,
            core::format_args!($($arg)*)
        ); }
    };
}


#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}
