#![allow(dead_code)]

use core::arch::asm;

// BYTE
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al",
             in("dx") port,
             in("al") value)
    }
}

pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx",
             in("dx") port,
             out("al") value)
    }
    value
}

// LONG
pub unsafe fn outl(port: u16, value: u32) {
    unsafe {
        asm!("out dx, eax",
             in("dx") port,
             in("eax") value)
    }
}

pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        asm!("in eax, dx",
             in("dx") port,
             out("eax") value)
    }
    value
}
