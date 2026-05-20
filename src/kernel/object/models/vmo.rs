use alloc::sync::Arc;

use crate::{kernel::object::{invoke::{Invocation, InvocationError}, obj::KernelObject, op::VmoOp}, memory::vmo::Vmo};


#[derive(Debug)]
pub struct VmoObject {
    vmo: Arc<Vmo>,
}

impl KernelObject for VmoObject {
    fn invoke(&self, invocation: Invocation) -> Result<usize, InvocationError> {
        if let Invocation::Vmo(vmo_op) = invocation {
            match vmo_op {
                VmoOp::GetPage { offset } => { 
                    if offset >= self.vmo.size {
                        return Err(InvocationError::InvalidArgument);
                    }
                    Ok(self.vmo.get_page(offset))
                },
                VmoOp::Resize { new_size } => { 
                    self.vmo.resize(new_size);
                    Ok(0)
                },
                VmoOp::Clone { offset, len } => { 
                    let new_handle = self.vmo.clone_range(offset, len);
                    Ok(new_handle)
                },
            }
        } else {
            Err(InvocationError::UnsupportedOperation)
        }
    }

    fn type_name(&self) -> &'static str {
        "VMO"
    }
}
