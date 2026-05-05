#![no_std]
#![no_main]
mod arch;
mod drivers;
mod kernel;
mod boot;

use core::panic::PanicInfo; 
use core::arch::asm;
use simple_psf::Psf;
use simple_psf::ParseError;

pub use boot::*;

use drivers::serial::{
    init_serial, 
    log_to_serial,
};

use kernel::memory::pmm::Allocator;
use kernel::memory::paging::*;

use arch::x86_64::interrupts::gdt::init_gdt;
use arch::x86_64::interrupts::idt::init_idt;

use drivers::graphics::*;

use crate::kernel::lock::TicketLock;
use crate::kernel::memory::init_pmm::HHDMOFFSET;

static ALLOCATOR: TicketLock<Allocator> = TicketLock::new(Allocator::new());

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(text) = info.message().as_str() {
        log_to_serial("!!! KERNEL PANIC : ");
        log_to_serial(text);
        log_to_serial(" !!!");
    } else {
        log_to_serial("!!! KERNEL PANIC !!!");
    }
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
        }
    }
}

const FONT_DATA: &[u8] = include_bytes!("../build_deps/zap-ext-light16.psf");

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    if !BASE_REVISION.is_supported() {
        hcf();
    }

    unsafe {
        init_serial();
        log_to_serial("\x1B[2J\x1B[H");
        log_to_serial("INITIATING GDT... ");
        init_gdt();
        log_to_serial("INITIATING IDT... ");
        init_idt();
    }

    let font = match Psf::parse(FONT_DATA) {
        Ok(f) => f,
        Err(ParseError::HeaderMissing) => { panic!("FONT LOAD FAILED: HEADER MISSING") },
        Err(ParseError::InvalidMagicBytes) => { panic!("FONT LOAD FAILED: INVALID MAGIC BYTES") },
        Err(ParseError::UnknownVersion(_)) => { panic!("FONT LOAD FAILED: UNKNOWN VERSION") },
        Err(ParseError::GlyphTableTruncated {..}) => { panic!("FONT LOAD FAILED: GLYPH TABLE TRUNCATED") },
    };
    log_to_serial("FONT LOADED\n");

    let fb = if let Some(fb_response) = FRAMEBUFFER_REQUEST.response() {
        if let Some(fb) = fb_response.framebuffers().first() {
            fb
        } else { panic!("Cannot get framebuffer") }
    } else { panic!("Cannot get framebuffer") };

    writeline("Initiating PMM... ", 0, 0, &font, fb);
    
    let meta_addr = {
        let mut allocator = ALLOCATOR.lock();
        allocator.init();
        allocator.metadata_phys_addr
    };

    writeline("Physical Memory Allocator initiated. Metatdata stored at: ", 1, 0, &font, fb);
    writenumber(meta_addr as u64, 1, 60, &font, fb);

    let mut pager = Pager::init(&ALLOCATOR).expect("Failed to init pager");
    writeline("Switched CR3", 2, 0, &font, fb);
    writeline("Hello, world!", 3, 0, &font, fb);

    // testing out the pager
    {
        let test_virt_addr = VirtAddress::new(500, 0, 0, 0, 0);
        let test_phys_addr = { ALLOCATOR.lock().alloc(0).unwrap() as u64 };

        pager.map_page(test_virt_addr, test_phys_addr, 0x7, *HHDMOFFSET as u64).expect("Failed to map test page");
        writeline("Successfully mapped virtual page at ", 4, 0, &font, fb); 
        writenumber(test_virt_addr.0, 4, 38, &font, fb);
        writeline("To physical page at: ", 5, 0, &font, fb);
        writenumber(test_phys_addr, 5, 22, &font, fb);

        writeline("Writing to test page...", 7, 0, &font, fb);
        unsafe {
            let ptr = test_virt_addr.0 as *mut u64;
            *ptr = 0xCAFEBABE_DEADBEEF;
        }

        writeline("Reading from test page... ", 8, 0, &font, fb);
        unsafe {
            let val = *(test_virt_addr.0 as *const u64);
            if val == 0xCAFEBABE_DEADBEEF { writeline("read successful!", 8, 28, &font, fb); }
        }

        writeline("Reading directly from physical address...", 9, 0, &font, fb);
        unsafe {
            let hhdm_ptr = (test_phys_addr + *HHDMOFFSET as u64) as *const u64;
            let hhdm_val = *hhdm_ptr;
            if hhdm_val == 0xCAFEBABE_DEADBEEF { writeline("read successful!", 9, 43, &font, fb); }
        }
    }

    hcf();
}
