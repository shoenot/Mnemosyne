use alloc::{
    alloc::{
        Layout,
        alloc,
        dealloc,
    },
    boxed::Box,
    vec::Vec,
};
use core::{
    hint::black_box,
    ptr::{
        read_volatile,
        write_volatile,
    },
};

use crate::{
    vklog,
    vklogln,
    klog,
    klogln,
};

pub fn test_kmalloc(print: bool) {
    unsafe {
        vklogln!(print, "");
        vklog!(print, "Running kmalloc tests... ");
        let layout = Layout::new::<u64>();
        let p1 = black_box(alloc(layout) as *mut u64);
        vklog!(print, "Allocation OK... ");

        if p1.is_null() {
            vklogln!(print, "[FAIL] p1 is null");
            panic!("MEMORY TEST FAILED");
        }

        write_volatile(p1, 0x12345678_ABCDEF01);
        if read_volatile(p1) != 0x12345678_ABCDEF01 {
            vklogln!(print, "[FAIL] Memory corruption at {:p}", p1);
            panic!("MEMORY TEST FAILED");
        }
        vklog!(print, "Write test OK... ");

        let original_addr = p1 as usize;
        dealloc(black_box(p1 as *mut u8), layout);

        let p2 = black_box(alloc(layout) as *mut u64);
        if p2 as usize != original_addr {
            vklogln!(print, "[FAIL] SLUB did not recycle pointer");
            panic!("MEMORY TEST FAILED");
        } else {
            vklogln!(print, "Recycling test OK");
        }

        dealloc(black_box(p2 as *mut u8), layout);
        vklogln!(print, "All kmalloc tests passed!");
    }
}

pub fn test_vmalloc(print: bool) {
    unsafe {
        vklogln!(print, "");
        vklog!(print, "Running vmalloc tests... ");

        let size = 8192; // 2 pages
        let layout = Layout::from_size_align(size, 4096).unwrap();
        let p_large = black_box(alloc(layout));

        if p_large.is_null() {
            vklogln!(print, "[FAIL] vmalloc failed for 8KB");
            panic!("MEMORY TEST FAILED");
        }

        if (p_large as usize) < 0x4000_0000 {
            vklog!(print, "[FAIL] vmalloc returned HHDM address instead of VMM address\n");
            panic!("MEMORY TEST FAILED");
        }
        vklog!(print, "Allocation OK... ");

        write_volatile(p_large as *mut u64, 0xAAAA_BBBB);
        if read_volatile(p_large as *mut u64) != 0xAAAA_BBBB {
            vklog!(print, "[FAIL] Demand paging failed");
            panic!("MEMORY TEST FAILED");
        }
        vklogln!(print, "Demand paging OK");

        black_box(dealloc(p_large, layout));
        vklogln!(print, "All vmalloc tests passed!");
    }
}

pub fn test_collections(print: bool) {
    vklogln!(print, "");
    vklogln!(print, "Testing rust high-level collections... ");

    vklog!(print, "    Testing boxes... ");
    let b = Box::new(42u32);
    if *b != 42 {
        vklogln!(print, "[FAIL] Box value mismatch");
        panic!("MEMORY TEST FAILED");
    }
    vklogln!(print, "Box test OK");

    vklog!(print, "    Testing vectors... ");
    let mut v = Vec::new();
    for i in 0..100 {
        v.push(i);
    }

    if v.len() != 100 || v[99] != 99 {
        vklogln!(print, "[FAIL] Vector corruption");
        panic!("MEMORY TEST FAILED");
    }
    vklogln!(print, "Vector test OK");

    vklogln!(print, "Collections tests passed!");
}
