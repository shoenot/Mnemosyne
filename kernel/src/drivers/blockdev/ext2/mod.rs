use core::ptr;

use alloc::{sync::Arc, vec::Vec};

use crate::{core::sync::TicketLock, drivers::blockdev::{AsyncBlockDevice, ext2::structs::{DiskGroupDesc, DiskSuperblock}}, memory::{ALLOCATOR, BlockSize, HHDMOFFSET, calculate_order}};

mod structs;

pub struct Ext2FileSystem {
    pub partition: Arc<dyn AsyncBlockDevice>,

    pub block_size: u32,
    pub sectors_per_block: u32,
    pub inodes_per_group: u32,
    pub blocks_per_group: u32,
    pub inode_size: u32,

    pub bgdt: TicketLock<Vec<DiskGroupDesc>>,
}

impl Ext2FileSystem {
    pub async fn mount(partition: Arc<dyn AsyncBlockDevice>) -> Result<Self, ()> {
        let page_phys = ALLOCATOR.alloc(BlockSize::Normal);
        if page_phys == 0 { return Err(()); }
        let page_virt = page_phys + *HHDMOFFSET;
        
        // read the ext2 superblock, which sits at 1024 bytes from part start
        let sb_future = partition.read_sectors(2, 2, page_phys as u64)?;
        sb_future.await?;

        let sb = unsafe { &*(page_virt as *const DiskSuperblock) };

        if sb.magic != 0xEF53 {
            ALLOCATOR.free(page_phys, BlockSize::Normal);
            return Err(());
        }

        let block_size = 1024 << sb.log_block_size;
        let sectors_per_block = block_size / 512;

        let inode_size = if sb.rev_level >= 1 {
            sb.inode_size
        } else {
            128
        };

        let total_blocks = sb.blocks_count;
        let num_groups = (total_blocks + sb.blocks_per_group - 1) / sb.blocks_per_group;
        let bgdt_bytes = num_groups as usize * size_of::<DiskGroupDesc>();
        let bgdt_blocks = (bgdt_bytes as u32 * block_size - 1) / block_size;
        let bgdt_sectors = bgdt_blocks * sectors_per_block;

        let bgdt_start_block = if block_size == 1024 { 2 } else { 1 };
        let bgdt_start_sector = bgdt_start_block as u64 * sectors_per_block as u64;

        let bgdt_alloc_bytes = (bgdt_sectors as usize * 512 + 4095);
        let bgdt_alloc_order = calculate_order(bgdt_alloc_bytes);
        let bgdt_buf_phys = match ALLOCATOR.alloc_order(bgdt_alloc_order) {
            Some(a) => a,
            None => {
                ALLOCATOR.free(page_phys, BlockSize::Normal);
                return Err(());
            }
        };
        let bgdt_buf_virt = bgdt_buf_phys + *HHDMOFFSET;

        let bgdt_future = partition.read_sectors(bgdt_start_sector, bgdt_sectors, bgdt_buf_phys as u64)?;
        bgdt_future.await?;

        let mut bgdt_vec = Vec::with_capacity(num_groups as usize);
        unsafe {
            let src_ptr = bgdt_buf_virt as *const DiskGroupDesc;
            for i in 0..num_groups as usize {
                bgdt_vec.push(ptr::read(src_ptr.add(i)));
            }
        }

        ALLOCATOR.free(page_phys, BlockSize::Normal);
        ALLOCATOR.free_order(bgdt_buf_phys, bgdt_alloc_order);

        Ok(Ext2FileSystem { 
            partition, 
            block_size, 
            sectors_per_block, 
            inodes_per_group: sb.inodes_per_group, 
            blocks_per_group: sb.blocks_per_group,
            inode_size: sb.inode_size as u32,
            bgdt: TicketLock::new(bgdt_vec) 
        })
    }

    pub async fn read_block(&self, block_id: u32, dest_phys: u64) -> Result<(), ()> {
        if block_id == 0 {
            unsafe {
                let dest_virt = dest_phys + *HHDMOFFSET as u64;
                ptr::write_bytes(dest_virt as *mut u8, 0, self.block_size as usize);
            }
            return Ok(());
        }

        let start_sector = block_id as u64 * self.sectors_per_block as u64;
        let future = self.partition.read_sectors(start_sector, self.sectors_per_block, dest_phys)?;
        future.await?;
        Ok(())
    }
}
