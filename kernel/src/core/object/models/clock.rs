use crate::core::{object::{invoke::InvocationError, obj::KernelObject}, time::get_realtime};
use async_trait::async_trait;
use vespertine_abi::Invocation;
use alloc::boxed::Box;
use vespertine_abi::op::ClockOp;

#[derive(Debug)]
pub struct Clock {}

#[async_trait]
impl KernelObject for Clock {
    fn type_name(&self) -> &'static str {
        "Clock"
    }

    async fn invoke(&self, invocation: Invocation, _calling_rights: crate::core::object::handle::AccessRights) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::Clock(ClockOp::GetTimestamp) =>  { 
                Ok(get_realtime() as usize) 
            },
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}
