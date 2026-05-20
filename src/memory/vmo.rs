use alloc::{collections::btree_map::BTreeMap, sync::Arc};

use crate::{kernel::sync::TicketLock, memory::GLOBAL_PMM};

#[derive(Debug)]
pub struct Vmo {
    pub size: usize,
    pub pages: TicketLock<BTreeMap<usize, usize>>,
}

impl Vmo {
    pub fn new(size: usize) -> Arc<Self> {
        Arc::new(Self {
            size, 
            pages: TicketLock::new(BTreeMap::new()),
        })
    }

    pub fn resize(&self, new_size: usize) {
        unimplemented!()
    }

    pub fn clone_range(&self, offset: usize, len: usize) -> usize {
        unimplemented!()
    }

    // for demand paging
    pub fn get_page(&self, offset: usize) -> usize {
        let mut pages = self.pages.lock();

        // return page if its already allocated
        if let Some(&pfn) = pages.get(&offset) {
            return pfn;
        }

        let pfn = GLOBAL_PMM.lock().alloc(super::BlockSize::Normal)
            .expect("Out of physical memory!");

        pages.insert(offset, pfn);
        pfn
    }
}
