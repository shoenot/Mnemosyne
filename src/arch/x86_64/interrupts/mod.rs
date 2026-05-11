pub mod gdt;
pub mod handle;
pub mod idt;

use core::arch::asm;

#[inline]
pub fn disable_interrupts() {
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
}

#[inline]
pub fn enable_interrupts() {
    unsafe {
        asm!("sti", options(nomem, nostack));
    }
}

#[inline]
pub fn interrupts_enabled() -> bool {
    let rflags: usize;
    unsafe {
        asm!("pushf",
            "pop {}",
            out(reg) rflags,
            options(nomem, preserves_flags))
    }
    (rflags & (1 << 9)) != 0
}

