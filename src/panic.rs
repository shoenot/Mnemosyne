use core::arch::asm;
use core::panic::PanicInfo;

use crate::drivers::logger::LOGGER;
use crate::klogln;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe { LOGGER.force_unlock() };
    klogln!("!------------- KERNEL PANIC -------------!");
    klogln!("{}\n", info);
    hcf();
}

pub(super) fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
        }
    }
}
