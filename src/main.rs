#![no_std]
#![no_main]
mod arch;
mod drivers;
mod kernel;
mod boot;

use core::panic::PanicInfo; 
use core::arch::asm;
use core::fmt::Write;
use simple_psf::Psf;
use simple_psf::ParseError;

pub use boot::*;

use drivers::serial::{
    init_serial, 
    log_to_serial,
};

use arch::x86_64::interrupts::gdt::init_gdt;
use arch::x86_64::interrupts::idt::init_idt;

use drivers::graphics::*;

use kernel::lock::TicketLock;
use kernel::memory::pmm::*;
use kernel::memory::paging::*;
use kernel::memory::vmm::*;

use crate::drivers::serial::SerialWriter;
use crate::drivers::serial::log_u64_to_serial;
use crate::kernel::memory::bs_kmalloc::init_bootstrap_allocator;

static ALLOCATOR: TicketLock<Allocator> = TicketLock::new(Allocator::new());
static PAGER: TicketLock<Pager> = TicketLock::new(Pager::new(&ALLOCATOR));
static GLOBAL_VMM: TicketLock<VirtMemManager> = TicketLock::new(VirtMemManager::new(&PAGER, &ALLOCATOR));

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log_to_serial("!!! KERNEL PANIC : ");
    let mut writer = SerialWriter;
    let _ = write!(&mut writer, "{}\n", info);
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

struct Logger<'a> {
    graphics_writer: &'a mut GraphicsWriter<'a>,
    serial_writer: &'a mut SerialWriter,
}

impl<'a> core::fmt::Write for Logger<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.graphics_writer.write_str(s)?;
        self.serial_writer.write_str(s)?;
        Ok(())
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

    let mut graphics_writer = GraphicsWriter {
        current_line: 0,
        current_offset: 0,
        font: &font,
        fb: &fb
    };

    let mut serial_writer = SerialWriter;

    let mut logger = Logger {
        graphics_writer: &mut graphics_writer,
        serial_writer: &mut serial_writer,
    };


    write!(&mut logger, "Initiating PMM... ");
    
    // Inititate PMM
    {
        let mut allocator = ALLOCATOR.lock();
        allocator.init();
    }

    write!(&mut logger, "Physical Memory Allocator initiated.\n");

    // Inititate Pager
    {
        let mut pager = PAGER.lock();
        pager.init();
    }

    // Initiate Bootstrap allocator 
    init_bootstrap_allocator();

    write!(&mut logger, "Switched CR3\n");

    write!(&mut logger, "Requesting 8KB of standard memory...\n");
    let standard_addr = GLOBAL_VMM.lock().mmap(0x2000, VM_FLAG_WRITE)
        .expect("\nFailed to mmap standard pages");
    
    let standard_ptr = standard_addr as *mut u8;
    
    unsafe {
        write!(&mut logger, "Writing to 0x{:x}\n", standard_addr as u64);
        *standard_ptr = 0xBE; 
        
        write!(&mut logger, "Writing to second page 0x{:x}\n", standard_addr as u64 + 0x1000);
        *(standard_ptr.add(0x1000)) = 0xEF;

        write!(&mut logger, "Reading back values: [0]: 0x{:x}, [1]: 0x{:x}\n", *standard_ptr as u64, *standard_ptr.add(0x1000) as u64);
        assert_eq!(*standard_ptr, 0xBE);
        assert_eq!(*(standard_ptr.add(0x1000)), 0xEF);
    }
    write!(&mut logger, "Standard Demand Paging OK\n");

    write!(&mut logger, "Requesting 2MB of Huge Page memory...\n");
    let huge_addr = GLOBAL_VMM.lock().mmap(0x200_000, VM_FLAG_WRITE | VM_FLAG_HUGE)
        .expect("\nFailed to mmap huge page");
        
    let huge_ptr = huge_addr as *mut u64;
    
    unsafe {
        write!(&mut logger, "Writing to 0x{:x}\n", huge_addr as u64);
        *huge_ptr = 0xDEADBEEF_CAFEBABE;
        
        write!(&mut logger, "Reading back value: 0x{:x}\n", *huge_ptr as u64);
        assert_eq!(*huge_ptr, 0xDEADBEEF_CAFEBABE);
    }
    write!(&mut logger, "Huge Page Demand Paging OK\n");

    write!(&mut logger, "Testing munmap...\n");
    
    GLOBAL_VMM.lock().munmap(standard_addr, 0x2000).expect("Failed to unmap standard memory");
    GLOBAL_VMM.lock().munmap(huge_addr, 0x200_000).expect("Failed to unmap huge memory");
    
    write!(&mut logger, "Unmaps successful\n");

    write!(&mut logger, "--- All VMM TESTS PASSED ---");

    hcf();
}
