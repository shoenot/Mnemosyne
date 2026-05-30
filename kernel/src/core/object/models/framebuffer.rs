use alloc::boxed::Box;
use alloc::slice;
use alloc::sync::Arc;
use core::cmp;

use async_trait::async_trait;
use vespertine_abi::{
    AccessRights,
    FileOp,
    Invocation,
};

use crate::arch::x86_64::task::syscall::safe_copy_to;
use crate::core::object::invoke::InvocationError;
use crate::core::object::models::vmo::VmoObject;
use crate::core::object::obj::KernelObject;
use crate::drivers::video::FramebufferInfo;

#[derive(Debug)]
pub struct FramebufferDevice {
    pub vmo: Arc<VmoObject>,
    pub info: FramebufferInfo,
}

#[async_trait]
impl KernelObject for FramebufferDevice {
    fn type_name(&self) -> &'static str { "Framebuffer Device" }

    async fn invoke(&self, invocation: Invocation, calling_rights: AccessRights) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::File(FileOp::Read { offset, buffer_ptr, len }) => {
                if !calling_rights.contains(AccessRights::READ) {
                    return Err(InvocationError::AccessDenied);
                }

                let info_bytes =
                    unsafe { slice::from_raw_parts(&self.info as *const FramebufferInfo as *const u8, size_of::<FramebufferInfo>()) };

                if offset >= info_bytes.len() {
                    return Ok(0);
                }

                let bytes_available = info_bytes.len() - offset;
                let read_len = cmp::min(bytes_available, len);
                unsafe {
                    let src = info_bytes.as_ptr().add(offset);
                    if !safe_copy_to(buffer_ptr as *mut u8, src, read_len) {
                        return Err(InvocationError::InvalidPointer);
                    }
                }
                Ok(read_len)
            }
            // forward vmo ops straight to the framebuffer vmo
            Invocation::Vmo(op) => self.vmo.invoke(Invocation::Vmo(op), calling_rights).await,
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}
