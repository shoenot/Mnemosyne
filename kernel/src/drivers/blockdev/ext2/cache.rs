use core::ptr::copy_nonoverlapping;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use alloc::{sync::Arc, vec::Vec};
use alloc::vec;

use crate::memory::HHDMOFFSET;
use crate::{core::sync::TicketLock, drivers::blockdev::AsyncBlockDevice};

pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

#[derive(Debug)]
pub struct CacheEntry {
    block_id: Option<usize>,
    referenced: bool,
    dirty: bool,
    in_flight: bool,
    data: Vec<u8>,
}

#[derive(Debug)]
pub struct BlockCache {
    device: Arc<dyn AsyncBlockDevice>,
    block_size: usize,
    sectors_per_block: usize,
    inner: TicketLock<BlockCacheInner>
}

#[derive(Debug)]
pub struct BlockCacheInner {
    entries: Vec<CacheEntry>,
    clock_hand: usize,
}

impl BlockCache {
    pub fn new(device: Arc<dyn AsyncBlockDevice>, block_size: usize, num_entries: usize) -> Self {
        let sectors_per_block = block_size / 512;
        let mut entries = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            entries.push(CacheEntry {
                block_id: None,
                referenced: false,
                dirty: false,
                in_flight: false,
                data: vec![0; block_size],
            }); 
        }

        Self { 
            device, block_size, sectors_per_block,
            inner: TicketLock::new(BlockCacheInner { entries, clock_hand: 0 }),
        }
    }

    pub async fn read_block(&self, block_id: usize, dest_phys: u64) -> Result<(), ()> {
        let mut idx;
        
        loop {
            let mut hit_idx = None;
            let mut is_in_flight = false;
            
            {
                let inner = self.inner.lock(); 
                for (i, entry) in inner.entries.iter().enumerate() {
                    if entry.block_id == Some(block_id) {
                        hit_idx = Some(i);
                        is_in_flight = entry.in_flight;
                        break;
                    }
                }
            }

            if let Some(i) = hit_idx {
                if is_in_flight {
                    // Block is currently in-flight (being loaded or written back).
                    // Yield and try again later.
                    yield_now().await;
                    continue;
                }

                // Cache Hit!
                let mut inner = self.inner.lock();
                let entry = &mut inner.entries[i];
                entry.referenced = true;

                unsafe {
                    let dest_virt = dest_phys + *HHDMOFFSET as u64;
                    copy_nonoverlapping(entry.data.as_ptr(), dest_virt as *mut u8, self.block_size);
                }
                return Ok(());
            }
            
            break; // Cache miss, proceed to allocation
        }

        // cache miss
        let mut selected_idx = None;
        {
            let mut inner = self.inner.lock();
            for (i, entry) in inner.entries.iter().enumerate() {
                if entry.block_id.is_none() && !entry.in_flight {
                    selected_idx = Some(i);
                    break;
                }
            }

            // second chance eviction if no vacant slot 
            if selected_idx.is_none() {
                let num_entries = inner.entries.len();
                let mut checked = 0;
                loop {
                    let i = inner.clock_hand;
                    
                    if inner.entries[i].in_flight {
                        inner.clock_hand = (inner.clock_hand + 1) % num_entries;
                        checked += 1;
                        if checked >= num_entries {
                            return Err(());
                        }
                        continue;
                    }

                    if !inner.entries[i].referenced {
                        selected_idx = Some(i);
                        inner.clock_hand = (inner.clock_hand + 1) % num_entries;
                        break;
                    } else {
                        inner.entries[i].referenced = false;
                        inner.clock_hand = (inner.clock_hand + 1) % num_entries;
                    }
                }
            }
            
            // Mark selected slot as in_flight under lock to reserve it
            if let Some(i) = selected_idx {
                inner.entries[i].in_flight = true;
            }
        }
        
        idx = selected_idx.ok_or(())?;

        // writeback evicted if dirty
        let mut old_writeback = None;
        {
            let inner = self.inner.lock();
            let entry = &inner.entries[idx];
            if entry.dirty {
                if let Some(old_block) = entry.block_id {
                    old_writeback = Some((old_block, entry.data.clone()));
                }
            }
        }

        if let Some((old_block, old_data)) = old_writeback {
            let start_sector = old_block as u64 * self.sectors_per_block as u64;
            let buffer_phys = old_data.as_ptr() as usize - *HHDMOFFSET;
            let write_future = self.device.write_sectors(start_sector, self.sectors_per_block as u32, buffer_phys as u64)?;
            write_future.await?;
            
            // Clear dirty flag now that writeback is complete
            {
                let mut inner = self.inner.lock();
                inner.entries[idx].dirty = false;
            }
        }

        // Now update block_id to reserve it for the new block before calling read_sectors
        {
            let mut inner = self.inner.lock();
            inner.entries[idx].block_id = Some(block_id);
        }

        // fetch new block from disk directly into evicted buffer 
        let new_sector = block_id as u64 * self.sectors_per_block as u64;
        let entry_phys = {
            let inner = self.inner.lock();
            inner.entries[idx].data.as_ptr() as usize - *HHDMOFFSET
        };

        let read_future = self.device.read_sectors(new_sector, self.sectors_per_block as u32, entry_phys as u64)?;
        read_future.await?;

        // update cache and copy data to caller
        {
            let mut inner = self.inner.lock();
            let entry = &mut inner.entries[idx];
            entry.referenced = true;
            entry.in_flight = false;

            unsafe {
                let dest_virt = dest_phys + *HHDMOFFSET as u64;
                copy_nonoverlapping(entry.data.as_ptr(), dest_virt as *mut u8, self.block_size);
            }
        }

        Ok(())
    }

    pub async fn write_block(&self, block_id: usize, src_phys: u64) -> Result<(), ()> {
        let mut idx;
        
        loop {
            let mut hit_idx = None;
            let mut is_in_flight = false;
            
            {
                let inner = self.inner.lock();
                for (i, entry) in inner.entries.iter().enumerate() {
                    if entry.block_id == Some(block_id) {
                        hit_idx = Some(i);
                        is_in_flight = entry.in_flight;
                        break;
                    }
                }
            }

            if let Some(i) = hit_idx {
                if is_in_flight {
                    yield_now().await;
                    continue;
                }

                let mut inner = self.inner.lock();
                let entry = &mut inner.entries[i];
                entry.referenced = true;
                entry.dirty = true;

                unsafe {
                    let src_virt = src_phys + *HHDMOFFSET as u64;
                    copy_nonoverlapping(src_virt as *const u8, entry.data.as_mut_ptr(), self.block_size);
                }
                return Ok(());
            }
            
            break;
        }

        // cache miss
        let mut selected_idx = None;
        {
            let mut inner = self.inner.lock();
            for (i, entry) in inner.entries.iter().enumerate() {
                if entry.block_id.is_none() && !entry.in_flight {
                    selected_idx = Some(i);
                    break;
                }
            }

            if selected_idx.is_none() {
                let num_entries = inner.entries.len();
                let mut checked = 0;
                loop {
                    let i = inner.clock_hand;
                    
                    if inner.entries[i].in_flight {
                        inner.clock_hand = (inner.clock_hand + 1) % num_entries;
                        checked += 1;
                        if checked >= num_entries {
                            return Err(());
                        }
                        continue;
                    }

                    if !inner.entries[i].referenced {
                        selected_idx = Some(i);
                        inner.clock_hand = (inner.clock_hand + 1) % num_entries;
                        break;
                    } else {
                        inner.entries[i].referenced = false;
                        inner.clock_hand = (inner.clock_hand + 1) % num_entries;
                    }
                }
            }
            
            if let Some(i) = selected_idx {
                inner.entries[i].in_flight = true;
            }
        }

        idx = selected_idx.ok_or(())?;

        // if evicted was dirty, write it back
        let mut old_writeback = None;
        {
            let inner = self.inner.lock();
            let entry = &inner.entries[idx];
            if entry.dirty {
                if let Some(old_block) = entry.block_id {
                    old_writeback = Some((old_block, entry.data.clone()));
                }
            }
        }

        if let Some((old_block, old_data)) = old_writeback {
            let start_sector = old_block as u64 * self.sectors_per_block as u64;
            let buffer_phys = old_data.as_ptr() as usize - *HHDMOFFSET;
            let write_future = self.device.write_sectors(start_sector, self.sectors_per_block as u32, buffer_phys as u64)?;
            write_future.await?;
            
            {
                let mut inner = self.inner.lock();
                inner.entries[idx].dirty = false;
            }
        }

        // overwrite cache entry buffer and mark dirty 
        {
            let mut inner = self.inner.lock();
            let entry = &mut inner.entries[idx];
            entry.block_id = Some(block_id);
            entry.referenced = true;
            entry.dirty = true;
            entry.in_flight = false;

            unsafe {
                let src_virt = src_phys + *HHDMOFFSET as u64;
                copy_nonoverlapping(src_virt as *const u8, entry.data.as_mut_ptr(), self.block_size);
            }
        }

        Ok(())
    }

    pub async fn flush(&self) -> Result<(), ()> {
        let mut dirty_entries = Vec::new();
        {
            let inner = self.inner.lock();
            for entry in inner.entries.iter() {
                if entry.dirty && !entry.in_flight {
                    if let Some(block_id) = entry.block_id {
                        dirty_entries.push((block_id, entry.data.clone()));
                    }
                }
            }
        }

        for (block_id, data) in dirty_entries {
            let start_sector = block_id as u64 * self.sectors_per_block as u64;
            let buffer_phys = data.as_ptr() as usize - *HHDMOFFSET;
            let write_future = self.device.write_sectors(start_sector, self.sectors_per_block as u32, buffer_phys as u64)?;
            write_future.await?;
        }

        // clear dirty flags
        {
            let mut inner = self.inner.lock();
            for entry in inner.entries.iter_mut() {
                if entry.dirty && !entry.in_flight {
                    entry.dirty = false;
                }
            }
        }

        Ok(())
    }
}
