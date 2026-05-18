use crate::kernel::object::handle::HandleID;

#[repr(C)]
#[derive(Debug)]
pub enum ChannelMessage {
    PushSmall { data: [u8; 32], len: u8 },
    PushLarge { vmo_handle: HandleID, offset: usize, len: usize },
    Pull,
}

#[repr(C)]
#[derive(Debug)]
pub enum DirectoryMessage {
    Link { name: *const u8, name_len: usize, handle_id: HandleID },
    Unlink { name: *const u8, name_len: usize },
    Lookup { name: *const u8, name_len: usize },
}

