pub mod env;
pub mod parser;
use alloc::alloc::{
    Layout,
    alloc,
};
use core::fmt;
use core::ptr::{
    copy_nonoverlapping,
    write_bytes,
};
use core::slice::from_raw_parts;

use parser::*;
use vespertine_abi::{
    AccessRights,
    FileOp,
    HandleID,
    Invocation,
};

use crate::arch::get_core_data;
use crate::core::object::models::process::Process;
use crate::core::thread::{
    ThreadControlBlock,
    get_current_process,
};
use crate::memory::vmm::{
    VM_FLAG_EXEC,
    VM_FLAG_USER,
    VM_FLAG_WRITE,
    align_up,
};
use crate::memory::vmo::{
    PagedBackingStore,
    Vmo,
};
use crate::memory::{
    HHDMOFFSET,
    NORMAL_PAGE_SIZE,
};
use crate::{
    KERNEL_PROCESS,
    klogln,
};

#[derive(Debug)]
pub enum LoaderError {
    InvalidBuffer,
    InvalidMagicNumbers,
    NotAWashingMachine,
    Not64BitElf,
    UnsupportedElfType(u16),
    UnsupportedArch(u16),
    UnsupportedABI(u8),
    FileReadError,
}

impl fmt::Display for LoaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoaderError::InvalidBuffer => write!(f, "InvalidBuffer"),
            LoaderError::InvalidMagicNumbers => write!(f, "Invalid ELF Magic numbers"),
            LoaderError::NotAWashingMachine => write!(f, "Big endian not supported"),
            LoaderError::Not64BitElf => write!(f, "32 bit programs not supported"),
            LoaderError::UnsupportedElfType(t) => write!(f, "Unsupported ELF type: 0x{:X}", t),
            LoaderError::UnsupportedArch(t) => write!(f, "Unsupported architechture: 0x{:X}", t),
            LoaderError::UnsupportedABI(t) => write!(f, "Unsupported ABI: 0x{:X}", t),
            LoaderError::FileReadError => write!(f, "File read or map error"),
        }
    }
}

pub async fn load_elf(file_handle: HandleID, proc: &Process) -> Result<usize, LoaderError> {
    // IN USER THREAD CONTEXT
    let file_obj = get_current_process()
        .ok_or(LoaderError::FileReadError)?
        .proc_handles
        .read()
        .resolve(file_handle, AccessRights::READ)
        .map_err(|_| LoaderError::FileReadError)?;

    // SWITCH TO KERNEL PROCESS TEMPORARILY
    let current_thread = get_core_data().scheduler.get_current_thread();

    let thread_addr = current_thread as usize;
    let old_proc = unsafe { (*current_thread).process.clone() };

    unsafe {
        (*current_thread).process = KERNEL_PROCESS.get().unwrap().clone();
    }

    let file_size = file_obj.invoke(Invocation::File(FileOp::Stat), AccessRights::READ).await.map_err(|_| LoaderError::FileReadError)?;

    let file_layout = Layout::from_size_align(file_size, 8).map_err(|_| LoaderError::FileReadError)?;

    let buffer_ptr = unsafe { alloc(file_layout) as *mut u8 };

    // Store the buffer allocation pointer as a usize to prevent it crossing the await boundary
    let buf_addr = buffer_ptr as usize;

    let read_result =
        file_obj.invoke(Invocation::File(FileOp::Read { offset: 0, buffer_ptr: buffer_ptr as usize, len: file_size }), AccessRights::READ);

    // RESTORE USER PROCESS TO DROP PRIVILEGES
    let thread_ptr = thread_addr as *mut ThreadControlBlock;
    unsafe {
        (*thread_ptr).process = old_proc;
    }

    read_result.await.map_err(|_| LoaderError::FileReadError)?;

    let file_bytes = unsafe { from_raw_parts(buf_addr as *mut u8, file_size) };

    let header = Elf64_Ehdr::from_bytes(file_bytes)?;
    let ph_iter = header.prog_headers(file_bytes).unwrap();

    for ph in ph_iter {
        if ph.p_type == P_Type::PT_LOAD as u32 {
            klogln!(
                "[INFO] Mapping Segment: file offset 0x{:X} -> virt addr 0x{:X} file size: {}, mem_size: {}",
                ph.p_offset,
                ph.p_vaddr,
                ph.p_filesz,
                ph.p_memsz
            );

            let aligned_vaddr = (ph.p_vaddr & !0xFFF) as usize;
            let offset_in_first_page = (ph.p_vaddr & 0xFFF) as usize;
            let total_map_size = align_up(offset_in_first_page + ph.p_memsz as usize);
            let vmo = Vmo::new(total_map_size as usize);

            let mut page_offset = 0;
            while page_offset < total_map_size {
                let pfn = vmo.request_page(page_offset).map_err(|_| LoaderError::FileReadError)?;
                let hhdm_ptr = pfn + *HHDMOFFSET;
                unsafe { write_bytes(hhdm_ptr as *mut u8, 0, NORMAL_PAGE_SIZE) };

                let overlap_start = usize::max(page_offset, offset_in_first_page as usize);
                let overlap_end = usize::min(page_offset + NORMAL_PAGE_SIZE, offset_in_first_page + ph.p_filesz as usize);

                if overlap_start < overlap_end {
                    unsafe {
                        let dst = ((overlap_start - page_offset) + hhdm_ptr) as *mut u8;
                        let src = file_bytes.as_ptr().add(ph.p_offset as usize + (overlap_start - offset_in_first_page));
                        let len = overlap_end - overlap_start;
                        copy_nonoverlapping(src, dst, len);
                    }
                }
                page_offset += NORMAL_PAGE_SIZE;
            }

            let mut vm_flags = VM_FLAG_USER;
            if (ph.p_flags & PF_W) != 0 {
                vm_flags |= VM_FLAG_WRITE
            };
            if (ph.p_flags & PF_X) != 0 {
                vm_flags |= VM_FLAG_EXEC
            };

            proc.vmm.write().mmap_vmo_at(aligned_vaddr, total_map_size, vm_flags, vmo.clone()).ok_or(LoaderError::FileReadError)?;
        }
    }

    klogln!("[INFO] Ready to jump to entry 0x{:X}", header.e_entry);
    Ok(header.e_entry as usize)
}
