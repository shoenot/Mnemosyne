use core::arch::x86_64::_rdtsc;
use crate::kernel::time::ClockSource;

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub struct TSC {
    pub frequency: usize,
}

impl ClockSource for TSC {
    fn name(&self) -> &'static str { "TSC" }

    fn read_counter(&self) -> usize {
        read_tsc_direct()
    }

    fn frequency(&self) -> usize { self.frequency }
}

pub fn read_tsc_direct() -> usize {
    unsafe { _rdtsc() as usize }
}
