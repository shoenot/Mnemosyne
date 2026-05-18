use alloc::format;
use alloc::string::String;
use core::fmt;
use core::str::Utf8Error;

use crate::kernel::object::handle::AccessRights;
use crate::kernel::object::message::{
    ChannelMessage,
    DirectoryMessage,
};

#[derive(Debug)]
pub enum InvocationError {
    AccessDenied,
    InvalidHandle,
    InvalidArgument(String),
    UnsupportedOperation,
}

impl fmt::Display for InvocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessDenied => write!(f, "INVOCATION ERROR: Access denied."),
            Self::InvalidHandle => write!(f, "INVOCATION ERROR: Invalid handle."),
            Self::InvalidArgument(s) => write!(f, "INVOCATION ERROR: Invalid argument: {}", s),
            Self::UnsupportedOperation => write!(f, "INVOCATION ERROR: Unsupported operation."),
        }
    }
}

impl From<Utf8Error> for InvocationError {
    fn from(err: Utf8Error) -> Self { InvocationError::InvalidArgument(format!("Invalid UTF-8 bytes passed ({})", err)) }
}

#[repr(C)]
#[derive(Debug)]
pub enum Invocation {
    Ping,
    GetInfo,
    Channel(ChannelMessage),
    Directory(DirectoryMessage),
}

impl Invocation {
    pub fn required_rights(&self) -> AccessRights {
        match self {
            Invocation::Ping => AccessRights::READ,
            Invocation::GetInfo => AccessRights::READ,
            Invocation::Channel(ChannelMessage::PushSmall { .. }) => AccessRights::WRITE,
            Invocation::Channel(ChannelMessage::PushLarge { .. }) => AccessRights::WRITE,
            Invocation::Channel(ChannelMessage::Pull) => AccessRights::READ,
            Invocation::Directory(DirectoryMessage::Link { .. }) => AccessRights::WRITE,
            Invocation::Directory(DirectoryMessage::Unlink { .. }) => AccessRights::WRITE,
            Invocation::Directory(DirectoryMessage::Lookup { .. }) => AccessRights::READ,
        }
    }
}
