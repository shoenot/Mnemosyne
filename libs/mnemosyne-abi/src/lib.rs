mod op;

use core::fmt::Debug;
use core::derive;

#[repr(C)]
#[derive(Debug)]
pub enum Invocation {
    Ping,
    GetInfo,
    Channel(ChannelOp),
    Directory(DirectoryOp),
    File(FileOp),
    Vmo(VmoOp),
    Proc(ProcOp),
    ProcessManager(ProcManOp),
    MemoryManager(MemManOp),
    MemPool(MemPoolOp),
    Clock(ClockOp),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HandleID(pub usize);

define_bitflags! {
    pub struct AccessRights(u8) {
        READ            = 1 << 0;
        WRITE           = 1 << 1;
        EXECUTE         = 1 << 2;
        CREATE          = 1 << 3;
        MUTATE          = 1 << 4;
    }
}


