use core::sync::atomic::AtomicPtr;

pub mod hpet;
pub mod tsc;

pub static GET_TIME_FN: AtomicPtr<()> = AtomicPtr::new(uninit_time as *mut ());

pub type TimeFn = fn() -> usize;

fn uninit_time() -> usize { 0 }


