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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_get() {
        let sm = SecretMem::new(42u64);
        assert_eq!(*sm.get(), 42);
    }

    #[test]
    fn get_mut_and_set() {
        let mut sm = SecretMem::new(10u64);
        *sm.get_mut() = 20;
        assert_eq!(*sm.get(), 20);
    }

    #[test]
    fn new_default_value() {
        let sm = SecretMem::<u32>::new(0);
        assert_eq!(*sm.get(), 0);
    }

    #[test]
    fn works_with_u8() {
        let mut sm = SecretMem::new(0xFFu8);
        assert_eq!(*sm.get(), 0xFF);
        *sm.get_mut() = 0x00;
        assert_eq!(*sm.get(), 0x00);
    }

    #[test]
    fn works_with_i32() {
        let sm = SecretMem::new(-42i32);
        assert_eq!(*sm.get(), -42);
    }
}
