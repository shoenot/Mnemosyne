use alloc::sync::Arc;
use alloc::boxed::Box;
use async_trait::async_trait;
use core::fmt::Debug;

use crate::core::object::invoke::InvocationError;
use vespertine_abi::{AccessRights, Invocation};

#[async_trait]
pub trait KernelObject: Send + Sync + Debug {
    async fn invoke(&self, invocation: Invocation, calling_rights: AccessRights) 
        -> Result<usize, InvocationError>;

    fn type_name(&self) -> &'static str { "Unknown" }
}

#[derive(Debug)]
pub struct HandleEntry {
    pub rights: AccessRights,
    pub object: Arc<dyn KernelObject>,
}

