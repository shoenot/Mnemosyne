use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cmp::min;
use core::future::poll_fn;
use core::sync::atomic::{
    AtomicBool,
    Ordering,
};
use core::task::{
    Context,
    Poll,
    Waker,
};

use async_trait::async_trait;
use vespertine_abi::op::{
    FileOp,
    SocketOp,
};
use vespertine_abi::{
    AccessRights,
    HandleID,
    Invocation,
    Signal,
    WaitOp,
};

use crate::arch::x86_64::task::syscall::{
    safe_copy_from,
    safe_copy_to,
};
use crate::core::object::invoke::InvocationError;
use crate::core::object::obj::KernelObject;
use crate::core::sync::{
    Mutex,
    TicketLock,
};

const BUFFER_SIZE: usize = 4096;

#[derive(Debug)]
pub struct RingBuffer {
    data: [u8; BUFFER_SIZE],
    head: usize,
    tail: usize,
}

impl RingBuffer {
    pub const fn new() -> Self { Self { data: [0; BUFFER_SIZE], head: 0, tail: 0 } }

    pub fn is_empty(&self) -> bool { self.head == self.tail }

    pub fn is_full(&self) -> bool { ((self.head + 1) % BUFFER_SIZE) == self.tail }

    pub fn len(&self) -> usize { if self.head >= self.tail { self.head - self.tail } else { BUFFER_SIZE - (self.tail - self.head) } }

    pub fn available_space(&self) -> usize { if self.is_full() { 0 } else { BUFFER_SIZE - self.len() - 1 } }

    pub fn push_slice(&mut self, src: &[u8]) -> usize {
        let n = min(src.len(), self.available_space());
        for i in 0..n {
            self.data[self.head] = src[i];
            self.head = (self.head + 1) % BUFFER_SIZE;
        }
        n
    }

    pub fn pop_slice(&mut self, dst: &mut [u8]) -> usize {
        let n = min(dst.len(), self.len());
        for i in 0..n {
            dst[i] = self.data[self.tail];
            self.tail = (self.tail + 1) % BUFFER_SIZE;
        }
        n
    }
}

#[derive(Debug)]
pub struct SocketBus {
    pub buffer: Mutex<RingBuffer>,
    pub is_closed: AtomicBool,
    pub read_waker: TicketLock<Option<Waker>>,
    pub write_waker: TicketLock<Option<Waker>>,
}

impl SocketBus {
    pub fn new() -> Self {
        Self {
            buffer: Mutex::new(RingBuffer::new()),
            is_closed: AtomicBool::new(false),
            read_waker: TicketLock::new(None),
            write_waker: TicketLock::new(None),
        }
    }
}

#[derive(Debug)]
pub struct SocketEndpoint {
    pub read_bus: Arc<SocketBus>,
    pub write_bus: Arc<SocketBus>,
    pub is_nb: AtomicBool,
}

#[async_trait]
impl KernelObject for SocketEndpoint {
    fn type_name(&self) -> &'static str { "Socket" }

    async fn invoke(&self, invocation: Invocation, calling_rights: AccessRights) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::File(FileOp::Read { buffer_ptr, len, .. }) => {
                if !calling_rights.contains(AccessRights::READ) {
                    return Err(InvocationError::AccessDenied);
                }
                poll_fn(|cx| self.read_async(buffer_ptr as *mut u8, len, cx)).await
            }
            Invocation::File(FileOp::Write { buffer_ptr, len, .. }) => {
                if !calling_rights.contains(AccessRights::WRITE) {
                    return Err(InvocationError::AccessDenied);
                }
                poll_fn(|cx| self.write_async(buffer_ptr as *mut u8, len, cx)).await
            }
            Invocation::Socket(SocketOp::SetNB { nb }) => {
                if !calling_rights.contains(AccessRights::WRITE) {
                    return Err(InvocationError::AccessDenied);
                }
                self.is_nb.store(nb, Ordering::SeqCst);
                Ok(0)
            }
            Invocation::Wait(WaitOp::One(signal)) => {
                if !calling_rights.contains(AccessRights::READ) {
                    return Err(InvocationError::AccessDenied);
                }
                poll_fn(|cx| self.wait_for_signals_async(signal, cx)).await
            }
            Invocation::Wait(WaitOp::Many { items_ptr: _, count: _ }) => {
                // invoke through ProcessControlBlock::invoke, not here manually
                Err(InvocationError::UnsupportedOperation)
            }
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}

impl Drop for SocketEndpoint {
    fn drop(&mut self) {
        // mark the write bus as closed
        self.write_bus.is_closed.store(true, Ordering::SeqCst);

        // wake up any reading task waiting on the write bus
        if let Some(waker) = self.write_bus.read_waker.lock().take() {
            waker.wake();
        }

        // ditto for writing tasks
        if let Some(waker) = self.write_bus.write_waker.lock().take() {
            waker.wake();
        }
    }
}

impl SocketEndpoint {
    pub fn new_pair() -> (Arc<SocketEndpoint>, Arc<SocketEndpoint>) {
        let bus1 = Arc::new(SocketBus::new());
        let bus2 = Arc::new(SocketBus::new());

        let ep1 = Arc::new(SocketEndpoint { read_bus: bus1.clone(), write_bus: bus2.clone(), is_nb: AtomicBool::new(false) });

        let ep2 = Arc::new(SocketEndpoint { read_bus: bus2, write_bus: bus1, is_nb: AtomicBool::new(false) });

        (ep1, ep2)
    }

    fn read_async(&self, buffer_ptr: *mut u8, len: usize, cx: &mut Context<'_>) -> Poll<Result<usize, InvocationError>> {
        if len == 0 {
            return Poll::Ready(Ok(0));
        }
        let mut bus = self.read_bus.buffer.lock();
        if !bus.is_empty() {
            let mut temp_buf = [0u8; 512];
            let to_read = min(len, temp_buf.len());
            let count = bus.pop_slice(&mut temp_buf[..to_read]);

            if !safe_copy_to(buffer_ptr, temp_buf.as_ptr(), count) {
                return Poll::Ready(Err(InvocationError::InvalidPointer));
            }

            if let Some(waker) = self.read_bus.write_waker.lock().take() {
                waker.wake();
            }

            return Poll::Ready(Ok(count));
        }

        if self.read_bus.is_closed.load(Ordering::SeqCst) {
            return Poll::Ready(Ok(0));
        }

        if self.is_nb.load(Ordering::SeqCst) {
            return Poll::Ready(Err(InvocationError::WouldBlock));
        }

        *self.read_bus.read_waker.lock() = Some(cx.waker().clone());
        Poll::Pending
    }

    fn write_async(&self, buffer_ptr: *const u8, len: usize, cx: &mut Context<'_>) -> Poll<Result<usize, InvocationError>> {
        if self.write_bus.is_closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(InvocationError::UnsupportedOperation)); // Broken pipe
        }
        if len == 0 {
            return Poll::Ready(Ok(0));
        }

        let mut temp_buf = [0u8; 512];
        let to_write = min(len, temp_buf.len());
        if !safe_copy_from(temp_buf.as_mut_ptr(), buffer_ptr, to_write) {
            return Poll::Ready(Err(InvocationError::InvalidPointer));
        }

        let mut bus = self.write_bus.buffer.lock();

        if self.write_bus.is_closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(InvocationError::UnsupportedOperation));
        }

        if bus.is_full() {
            if self.is_nb.load(Ordering::SeqCst) {
                return Poll::Ready(Err(InvocationError::WouldBlock));
            }
            *self.write_bus.write_waker.lock() = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let count = bus.push_slice(&temp_buf[..to_write]);

        if let Some(waker) = self.write_bus.read_waker.lock().take() {
            waker.wake();
        }

        Poll::Ready(Ok(count))
    }

    fn wait_for_signals_async(&self, signal: Signal, cx: &mut Context<'_>) -> Poll<Result<usize, InvocationError>> {
        let mut should_block = false;
        let mut is_write = false;

        if signal.contains(Signal::READABLE) {
            let bus = self.read_bus.buffer.lock();
            if bus.is_empty() && !self.read_bus.is_closed.load(Ordering::SeqCst) {
                should_block = true;
                is_write = false;
            }
            drop(bus);
        }

        if signal.contains(Signal::WRITABLE) {
            let bus = self.write_bus.buffer.lock();
            if bus.is_full() && !self.write_bus.is_closed.load(Ordering::SeqCst) {
                should_block = true;
                is_write = true;
            }
            drop(bus);
        }

        if signal.contains(Signal::PEER_CLOSED) {
            let bus = self.read_bus.buffer.lock();
            if !self.read_bus.is_closed.load(Ordering::SeqCst) {
                should_block = true;
                is_write = false;
            }
            drop(bus);
        }

        if !should_block {
            return Poll::Ready(Ok(0));
        }

        if is_write {
            *self.write_bus.write_waker.lock() = Some(cx.waker().clone());
        } else {
            *self.read_bus.read_waker.lock() = Some(cx.waker().clone());
        }

        Poll::Pending
    }
}

#[derive(Debug)]
pub struct SocketFactory {}

#[async_trait]
impl KernelObject for SocketFactory {
    fn type_name(&self) -> &'static str { "SocketFactory" }

    async fn invoke(&self, invocation: Invocation, calling_rights: AccessRights) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::Socket(SocketOp::Create { .. }) => {
                if !calling_rights.contains(AccessRights::CREATE) {
                    return Err(InvocationError::AccessDenied);
                }
                let (ep1, ep2) = SocketEndpoint::new_pair();
                let current_proc = crate::core::thread::get_current_process().ok_or(InvocationError::OutOfMemory)?;

                let mut handles = current_proc.proc_handles.write();
                let h1 = handles.insert(ep1, AccessRights::all());
                let h2 = handles.insert(ep2, AccessRights::all());

                // Pack both handles into return value: low 32 = h1, high 32 = h2
                Ok((h1.0 & 0xFFFFFFFF) | ((h2.0 & 0xFFFFFFFF) << 32))
            }
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}

pub fn init_ipc_pipeline() -> (HandleID, HandleID) {
    let (ep1, ep2) = SocketEndpoint::new_pair();
    let current_proc = crate::core::thread::get_current_process().expect("No current process during IPC init");
    let mut handles = current_proc.proc_handles.write();
    let h1 = handles.insert(ep1, AccessRights::all());
    let h2 = handles.insert(ep2, AccessRights::all());
    (h1, h2)
}
