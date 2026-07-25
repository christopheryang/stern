#![allow(unsafe_code)]

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ptr;
use zeroize::Zeroize;

pub struct SecretMem<T: Copy + Default + Zeroize> {
    ptr: *mut T,
    layout: Layout,
}

unsafe impl<T: Copy + Default + Zeroize + Send> Send for SecretMem<T> {}
unsafe impl<T: Copy + Default + Zeroize + Sync> Sync for SecretMem<T> {}

impl<T: Copy + Default + Zeroize> SecretMem<T> {
    pub fn new(initial: T) -> Self {
        let layout = Layout::new::<T>();
        let ptr = unsafe { alloc_zeroed(layout).cast::<T>() };
        if ptr.is_null() {
            std::process::abort();
        }
        unsafe {
            ptr::write(ptr, initial);
        }
        Self { ptr, layout }
    }

    #[must_use]
    pub fn get(&self) -> &T {
        unsafe { &*self.ptr }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.ptr }
    }
}

impl<T: Copy + Default + Zeroize> Drop for SecretMem<T> {
    fn drop(&mut self) {
        unsafe {
            let slice = std::slice::from_raw_parts_mut(self.ptr.cast::<u8>(), self.layout.size());
            slice.zeroize();
            dealloc(self.ptr.cast::<u8>(), self.layout);
        }
    }
}
