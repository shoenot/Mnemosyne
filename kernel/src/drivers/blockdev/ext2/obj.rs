use alloc::boxed::Box;
use alloc::sync::Arc;
use core::ptr::copy_nonoverlapping;

use async_trait::async_trait;
use vespertine_abi::protocol::{
    AbiDirEntry,
    DirEntryType,
    PacketFlags,
    PacketHeader,
    VESPER_MAGIC,
};
use vespertine_abi::{
    AccessRights,
    DirectoryOp,
    FileOp,
    Invocation,
};

use crate::arch::x86_64::task::syscall::safe_copy_to;
use crate::core::object::invoke::InvocationError;
use crate::core::object::models::directory::Filename;
use crate::core::object::models::vmo::VmoObject;
use crate::core::object::obj::KernelObject;
use crate::core::sync::TicketLock;
use crate::core::thread::get_current_process;
use crate::drivers::blockdev::ext2::Ext2FileSystem;
use crate::drivers::blockdev::ext2::structs::{
    DiskDirHeader,
    DiskInode,
};
use crate::memory::vmo::{
    FileVmo,
    PagedBackingStore,
};
use crate::memory::{
    ALLOCATOR,
    BlockSize,
    HHDMOFFSET,
};

#[derive(Debug)]
pub struct Ext2File {
    pub fs: Arc<Ext2FileSystem>,
    pub inode_num: u32,
    pub inode_data: DiskInode,
    pub file_vmo: Arc<FileVmo>,
    pub offset: TicketLock<usize>,
}

unsafe impl Send for Ext2File {}
unsafe impl Sync for Ext2File {}

#[async_trait]
impl KernelObject for Ext2File {
    fn type_name(&self) -> &'static str { "File" }

    async fn invoke(&self, invocation: Invocation, _rights: AccessRights) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::File(FileOp::Read { offset: _, buffer_ptr, len }) => {
                let mut offset_guard = self.offset.lock();
                let current_offset = *offset_guard;
                let bytes_read = self.read_bytes_async(current_offset, buffer_ptr, len).await?;

                *offset_guard += bytes_read;
                Ok(bytes_read)
            }
            Invocation::File(FileOp::Stat) => Ok(self.inode_data.size as usize),
            Invocation::File(FileOp::GetVmo) => {
                let vmo_obj = Arc::new(VmoObject::new(self.file_vmo.clone()));
                let current_proc = get_current_process().ok_or(InvocationError::UnsupportedOperation)?;
                let handle_id = current_proc.proc_handles.write().insert(vmo_obj, AccessRights::all());

                Ok(handle_id.0 as usize)
            }
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}

impl Ext2File {
    async fn read_bytes_async(&self, offset: usize, buffer_ptr: usize, req_len: usize) -> Result<usize, InvocationError> {
        let file_size = self.inode_data.size as usize;
        if offset >= file_size {
            return Ok(0);
        };

        let bytes_available = file_size - offset;
        let read_len = core::cmp::min(bytes_available, req_len);
        if read_len == 0 {
            return Ok(0);
        }

        let mut bytes_copied = 0;

        while bytes_copied < read_len {
            let current_file_offset = offset + bytes_copied;
            let page_offset = (current_file_offset / 4096) * 4096;
            let block_internal_offset = current_file_offset % 4096;

            let phys_addr = self.file_vmo.request_page(page_offset).map_err(|_| InvocationError::InvalidPointer)?;

            let page_virt = phys_addr + *HHDMOFFSET;
            let chunk_size = core::cmp::min(4096 - block_internal_offset, read_len - bytes_copied);

            unsafe {
                let src_ptr = (page_virt as *const u8).add(block_internal_offset);
                let dst_ptr = (buffer_ptr as *mut u8).add(bytes_copied);

                if !safe_copy_to(dst_ptr, src_ptr, chunk_size) {
                    return Err(InvocationError::InvalidPointer);
                }
            }
            bytes_copied += chunk_size;
        }
        Ok(bytes_copied)
    }
}

#[derive(Debug)]
pub struct Ext2Directory {
    pub fs: Arc<Ext2FileSystem>,
    pub inode_num: u32,
    pub inode_data: DiskInode,
}

#[async_trait]
impl KernelObject for Ext2Directory {
    fn type_name(&self) -> &'static str { "Directory" }

    async fn invoke(&self, invocation: Invocation, calling_rights: AccessRights) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::Directory(DirectoryOp::Lookup { name, name_len }) => {
                let filename = Filename::new(name as *const u8, name_len)?;

                let child_inode_id = self
                    .fs
                    .lookup_in_dir(&self.inode_data, &*filename.name)
                    .await
                    .map_err(|_| InvocationError::PathNotFound)?
                    .ok_or(InvocationError::PathNotFound)?;

                let child_inode_data = self.fs.read_inode(child_inode_id).await.map_err(|_| InvocationError::PathNotFound)?;

                let is_directory = (child_inode_data.mode & 0xF000) == 0x4000;

                let target_object: Arc<dyn KernelObject> = if is_directory {
                    Arc::new(Ext2Directory { fs: Arc::clone(&self.fs), inode_num: child_inode_id, inode_data: child_inode_data })
                } else {
                    let file_vmo =
                        FileVmo::new(Arc::clone(&self.fs), child_inode_id, child_inode_data.clone(), child_inode_data.size as usize);
                    Arc::new(Ext2File {
                        fs: Arc::clone(&self.fs),
                        inode_num: child_inode_id,
                        inode_data: child_inode_data,
                        file_vmo,
                        offset: TicketLock::new(0),
                    })
                };

                let rights = AccessRights(calling_rights.0 & (AccessRights::READ | AccessRights::WRITE | AccessRights::EXECUTE).0);

                let handle_id =
                    get_current_process().ok_or(InvocationError::InvalidHandle)?.proc_handles.write().insert(target_object, rights);

                Ok(handle_id.0)
            }
            Invocation::Directory(DirectoryOp::List { offset: _, sink }) => {
                let mut entries = alloc::vec::Vec::new();

                let page_phys = ALLOCATOR.alloc(BlockSize::Normal);
                if page_phys == 0 {
                    return Err(InvocationError::OutOfMemory);
                }
                let page_virt = page_phys + *HHDMOFFSET;

                // 1. Read all directory entries from Ext2 blocks
                for direct_idx in 0..12 {
                    let block_id = unsafe { self.inode_data.data.blocks.direct[direct_idx] };
                    if block_id == 0 {
                        continue;
                    };

                    if self.fs.read_block(block_id, page_phys as u64).await.is_err() {
                        ALLOCATOR.free(page_phys, BlockSize::Normal);
                        return Err(InvocationError::InvalidPointer);
                    }

                    let mut offset = 0;
                    while offset < self.fs.block_size as usize {
                        unsafe {
                            let entry_ptr = (page_virt as *const u8).add(offset) as *const DiskDirHeader;
                            let inode_id = (*entry_ptr).inode;
                            let rec_len = (*entry_ptr).record_length as usize;
                            let name_len = (*entry_ptr).name_length as usize;

                            if rec_len == 0 {
                                break;
                            }

                            if inode_id != 0 && name_len > 0 && offset + 8 + name_len <= self.fs.block_size as usize {
                                let name_ptr = (entry_ptr as *const u8).add(8);
                                let name_slice = core::slice::from_raw_parts(name_ptr, name_len);

                                if let Ok(entry_name) = core::str::from_utf8(name_slice) {
                                    if entry_name != "." && entry_name != ".." {
                                        entries.push((alloc::string::ToString::to_string(entry_name), (*entry_ptr).file_type));
                                    }
                                }
                            }
                            offset += rec_len;
                        }
                    }
                }
                ALLOCATOR.free(page_phys, BlockSize::Normal);

                // 2. Resolve the IPC sink socket to stream entries to userspace
                let proc = get_current_process().ok_or(InvocationError::InvalidHandle)?;
                let sink_obj = proc.proc_handles.read().resolve(sink, AccessRights::WRITE)?;

                let mut iter = entries.iter().peekable();
                while let Some((name_str, file_type)) = iter.next() {
                    let mut entry = AbiDirEntry {
                        entry_type: match *file_type {
                            2 => DirEntryType::Directory as u8,
                            1 => DirEntryType::File as u8,
                            _ => DirEntryType::Object as u8,
                        },
                        name_len: core::cmp::min(name_str.len(), 254) as u8,
                        name: [0u8; 254],
                    };
                    let len = entry.name_len as usize;
                    entry.name[..len].copy_from_slice(&name_str.as_bytes()[..len]);

                    let mut flags = PacketFlags::IS_STREAM;
                    if iter.peek().is_some() {
                        flags = flags.insert(PacketFlags::HAS_NEXT);
                    }

                    let header = PacketHeader {
                        magic: VESPER_MAGIC,
                        version: 1,
                        packet_flags: flags,
                        packet_type: 1,
                        payload_len: core::mem::size_of::<AbiDirEntry>() as u32,
                        reserved: 0,
                    };

                    let mut buffer = [0u8; core::mem::size_of::<PacketHeader>() + core::mem::size_of::<AbiDirEntry>()];
                    let header_size = core::mem::size_of::<PacketHeader>();
                    let entry_size = core::mem::size_of::<AbiDirEntry>();
                    unsafe {
                        let header_ptr = &header as *const _ as *const u8;
                        let entry_ptr = &entry as *const _ as *const u8;
                        copy_nonoverlapping(header_ptr, buffer.as_mut_ptr(), header_size);
                        copy_nonoverlapping(entry_ptr, buffer.as_mut_ptr().add(header_size), entry_size);
                    }

                    let op = FileOp::Write { offset: 0, buffer_ptr: buffer.as_mut_ptr() as usize, len: buffer.len() };
                    sink_obj.invoke(Invocation::File(op), AccessRights::WRITE).await?;
                }

                Ok(0)
            }
            _ => Err(InvocationError::UnsupportedOperation),
        }
    }
}
