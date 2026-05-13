pub mod x86_64;

use core::sync::atomic::Ordering;

use x86_64::{
    apic::lapic::init_local_apic,
    cpu::{
        core::{
            activate_core,
            init_core_data,
        },
        fpu::{
            init_cr4,
            init_default_fpu_cxt,
        },
    },
    init_global_apics,
    init_interrupts,
};

use crate::arch::x86_64::{
    apic::lapic::ApicDriver,
    cpu::fpu::{
        USE_XSAVE,
        init_xsave,
    },
};

pub fn init() { init_interrupts(); }

pub fn init_bootstrap_core() {
    init_global_apics();
    let lapic = init_local_apic();
    let lapic_id = lapic.id();
    let data_ptr = init_core_data(lapic_id as usize, lapic);
    activate_core(data_ptr);
}

pub fn init_fpu() {
    unsafe {
        init_cr4();
        init_default_fpu_cxt();
    }
}

pub fn init_ap_fpu() {
    unsafe {
        init_cr4();
        if USE_XSAVE.load(Ordering::Relaxed) {
            init_xsave();
        }
    }
}
