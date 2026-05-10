

enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

enum ThreadPriority {
    Idle,
    Low,
    Medium,
    High,
    Maximum,
}

#[repr(C)]
struct 

#[repr(C)]
struct ThreadControlBlock {
    thread_id: usize,
    state: ThreadState,
    priority: usize
}
