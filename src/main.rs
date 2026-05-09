#![no_std]
#![no_main]
mod arch;
mod drivers;
mod kernel;
mod boot;
mod tests;
mod panic;

extern crate alloc;

use drivers::logger::Logger;
pub use boot::*;

use panic::hcf;

use arch::x86_64::interrupts::gdt::init_gdt;
use arch::x86_64::interrupts::idt::init_idt;
use arch::x86_64::apic::lapic::{Local_APIC, get_apic_base};
use arch::x86_64::apic::ioapic::IoApic;
use arch::x86_64::timer;

use kernel::lock::TicketLock;

use kernel::memory::pmm::*;
use kernel::memory::paging::*;
use kernel::memory::vmm::*;
use kernel::memory::heap::KernelAllocator;

use kernel::acpi;

use tests::memory_tests::*;

#[global_allocator]
pub static KERNEL_ALLOCATOR: KernelAllocator = KernelAllocator::new();

static ALLOCATOR: TicketLock<Allocator> = TicketLock::new(Allocator::new());
static PAGER: TicketLock<Pager> = TicketLock::new(Pager::new(&ALLOCATOR));
static GLOBAL_VMM: TicketLock<VirtMemManager> = TicketLock::new(VirtMemManager::new(&PAGER, &ALLOCATOR));

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    if !BASE_REVISION.is_supported() {
        hcf();
    }

    unsafe {
        klog!("INITIATING GDT...");
        init_gdt();
        klogln!("GDT INIT OK.");
        klog!("INITIATING IDT...");
        init_idt();
        klogln!("IDT INIT OK.");
    }
    
    klogln!("INITIATING MEMORY MANAGERS... ");
    
    // Inititate PMM
    {
        let mut allocator = ALLOCATOR.lock();
        allocator.init();
    }

    // Inititate Pager
    {
        let mut pager = PAGER.lock();
        pager.init();
    }

    klogln!("SWITCHED CR3. PAGING HANDOVER COMPLETE.");
    
    klogln!("RUNNING MEMORY TESTS");
    
    test_kmalloc();
    test_vmalloc();
    test_collections();

    klogln!("TESTS COMPLETE!");
    
    unsafe {
        let apic_phys = get_apic_base() as u64;
        let apic_virt = apic_phys + *HHDMOFFSET as u64;
        let mut pager = PAGER.lock();
        let flags = get_flags(true, true, false, true, true, false, false, false, true, true);
        pager.map_page(VirtAddress(apic_virt), apic_phys, flags, *HHDMOFFSET as u64, BlockSize::Normal).unwrap();
        drop(pager);
    }

    let lapic = Local_APIC::init();
    let lapic_id = lapic.id();
    // lapic.timer_setup(32, 0x0FFF_FFFF);

    let ioapic = IoApic::new();
    {
        let ioapic_virt = ioapic.base_addr as u64;
        let ioapic_phys = ioapic_virt - *HHDMOFFSET as u64;
        let mut pager = PAGER.lock();
        let flags = get_flags(true, true, false, true, true, false, false, false, true, true);
        pager.map_page(VirtAddress(ioapic_virt), ioapic_phys, flags, *HHDMOFFSET as u64, BlockSize::Normal).unwrap();
        drop(pager);
    }

    let rsdp = acpi::rsdp::Rsdp::get();
    let sdt = acpi::sdt::SDTArray::get(rsdp.get_table());
    let madt_info = acpi::madt::parse_madt(&sdt);

    let mut pit_gsi = 0;
    for ovr in madt_info.overrides.iter() {
        if ovr.source == 0 {
            pit_gsi = ovr.gsi;
            break;
        }
    }

    ioapic.set_entry(pit_gsi, 32, lapic_id);
    timer::pit::init(1000);

    hcf();
}
