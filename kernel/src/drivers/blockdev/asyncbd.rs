use crate::drivers::virtio::blk::BlockTransferFuture;

pub trait AsyncBlockDevice: Send + Sync {
    fn read_sectors(&self, 
        sector: u64, 
        sectors_count: u32, 
        buf_phys: u64) -> Result<BlockTransferFuture, ()>;

    fn write_sectors(&self,
        sector: u64,
        sectors_count: u32,
        buf_phys: u64) -> Result<BlockTransferFuture, ()>;

    fn sector_size(&self) -> usize { 512 }
}
