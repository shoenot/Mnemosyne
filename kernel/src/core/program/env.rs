use alloc::sync::Arc;
use core::ptr::{
    copy_nonoverlapping,
    null,
    write,
};

use vespertine_abi::{
    HandleGrant,
    ProcessInitPackage,
};
use vespertine_common::slab::NORMAL_PAGE_SIZE;

use crate::core::object::invoke::InvocationError;
use crate::memory::HHDMOFFSET;
use crate::memory::vmo::{
    PagedBackingStore,
    Vmo,
};

pub struct ProcessEnvironment;

impl ProcessEnvironment {
    pub fn inject(
        stack_vmo: &Arc<Vmo>, stack_vaddr: usize, stack_size: usize, extra_handles: &[HandleGrant], args_buffer: &[u8], argc: usize,
        mut initpkg: ProcessInitPackage,
    ) -> Result<(usize, usize), InvocationError> {
        let top_page_offset = stack_size - NORMAL_PAGE_SIZE;
        let phys_frame = stack_vmo.request_page(top_page_offset).map_err(|_| InvocationError::OutOfMemory)?;

        // calculate sizes
        let initpkg_size = size_of::<ProcessInitPackage>();
        let handles_array_size = extra_handles.len() * size_of::<HandleGrant>();
        let argv_array_size = (argc + 1) * size_of::<*const u8>(); // +1 for null terminator
        let strings_size = args_buffer.len();

        let total_payload_size = initpkg_size + handles_array_size + argv_array_size + strings_size;
        if total_payload_size > NORMAL_PAGE_SIZE {
            return Err(InvocationError::OutOfMemory);
        }

        // calculate offsets
        let base_offset = (NORMAL_PAGE_SIZE - total_payload_size) & !0xF;

        let pkg_offset = base_offset;
        let handles_offset = pkg_offset + initpkg_size;
        let argv_offset = handles_offset + handles_array_size;
        let strings_offset = argv_offset + argv_array_size;

        // hhdm ptrs
        let hhdm_addr = phys_frame + *HHDMOFFSET;

        let pkg_hhdm_ptr = (hhdm_addr + pkg_offset) as *mut ProcessInitPackage;
        let handles_hhdm_ptr = (hhdm_addr + handles_offset) as *mut HandleGrant;
        let argv_hhdm_ptr = (hhdm_addr + argv_offset) as *mut *const u8;
        let strings_hhdm_ptr = (hhdm_addr + strings_offset) as *mut u8;

        // virt addrs
        let base_vaddr = stack_vaddr + top_page_offset;
        let pkg_vaddr = base_vaddr + pkg_offset;
        let handles_vaddr = base_vaddr + handles_offset;
        let argv_vaddr = base_vaddr + argv_offset;
        let strings_vaddr = base_vaddr + strings_offset;

        unsafe {
            // copy raw strings buffer
            if strings_size > 0 {
                copy_nonoverlapping(args_buffer.as_ptr(), strings_hhdm_ptr, strings_size);
            }

            // build argv pointer array
            let mut current_string_vaddr = strings_vaddr;
            let mut arg_idx = 0;

            let mut i = 0;
            let mut start = 0;
            while i < strings_size {
                if args_buffer[i] == 0 {
                    write(argv_hhdm_ptr.add(arg_idx), current_string_vaddr as *const u8);
                    arg_idx += 1;

                    let str_len = (i - start) + 1;
                    current_string_vaddr += str_len;
                    start = i + 1;
                }
                i += 1
            }

            // null terminate argv array
            write(argv_hhdm_ptr.add(argc), null());

            // copy handle grants
            core::ptr::copy_nonoverlapping(extra_handles.as_ptr(), handles_hhdm_ptr, extra_handles.len());

            // populate and write init package
            initpkg.extra_handles_ptr = handles_vaddr as *const HandleGrant;
            initpkg.argc = argc;
            initpkg.argv = argv_vaddr as *const *const u8;
            core::ptr::write(pkg_hhdm_ptr, initpkg);
        }

        // sysv abi requires rsp+8 to be 16 byte aligned at func entry
        let safe_stack_top = pkg_vaddr - 8;
        Ok((pkg_vaddr, safe_stack_top))
    }
}
