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

use arch::x86_64::interrupts::gdt::init_gdt;
use arch::x86_64::interrupts::idt::init_idt;

use drivers::graphics::*;

use crate::kernel::lock::TicketLock;

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
        log_to_serial("hello, world!\n");
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

    writeline("Hello, world!", 0, 0, &font, fb);

    writeline("Initiating PMM... ", 1, 0, &font, fb);
    
    let meta_addr = {
        let mut allocator = ALLOCATOR.lock();
        allocator.init();
        allocator.metadata_phys_addr
    };

    writeline("Physical Memory Allocator initiated. Metatdata stored at: ", 2, 0, &font, fb);
    writenumber(meta_addr as u64, 2, 60, &font, fb);

    writeline("test", 3, 0, &font, fb);
    hcf();
}
