use crate::constants::{ALLOC_ALIGNMENT, ENTRIES_INITIAL_CAPACITY};
use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;
use std::{mem, ptr, slice};

/// | hash (u64) | len (usize) | chars (len) |
///              ^
///           pointer
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) struct Entry(pub(crate) NonNull<u8>);

impl Entry {
    pub(crate) fn as_str<'a>(&self) -> &'a str {
        unsafe {
            let ptr = self.0.as_ptr() as *const usize;
            let str_len = ptr::read(ptr);
            let char_ptr = ptr.add(1) as *const u8;
            #[allow(clippy::transmute_bytes_to_str)]
            mem::transmute(slice::from_raw_parts(char_ptr, str_len))
        }
    }

    pub(crate) fn hash(&self) -> u64 {
        unsafe {
            let ptr = self.0.as_ptr() as *const u64;
            ptr::read(ptr.sub(1))
        }
    }
}

unsafe impl Sync for Entry {}
unsafe impl Send for Entry {}

/// Entries holds the allocated entries in hashmap.
pub(crate) struct Entries {
    data: NonNull<Option<Entry>>,
    /// bytes to the next growth
    ///   = size * 3 / 4 - items_count
    ///   = (mask + 1) / 4 * 3 - items_count
    /// Note that size should always be 4n
    growth_left: usize,
    ///   = size - 1
    mask: usize,
}

static DUMMY_ENTRY_SLOT: Option<Entry> = None;

impl Entries {
    pub(crate) fn new() -> Self {
        Self {
            data: unsafe {
                NonNull::new_unchecked(&DUMMY_ENTRY_SLOT as *const _ as *mut Option<Entry>)
            },
            growth_left: 0,
            mask: 0,
        }
    }

    pub(crate) fn get_or_insert<F>(
        &mut self,
        hash: u64,
        string: &str,
        mut entry_factory: F,
    ) -> Entry
    where
        F: FnMut() -> Entry,
    {
        if self.growth_left == 0 {
            unsafe { self.grow() }
        }

        debug_assert!(self.growth_left > 0);

        let mut pos = self.mask & hash as usize;
        let mut dist = 0;
        let slot = loop {
            match unsafe { &mut *self.data.as_ptr().add(pos) } {
                Some(entry) => {
                    if entry.hash() == hash && entry.as_str() == string {
                        return *entry;
                    }
                    dist += 1;
                    pos = (pos + dist) & self.mask;
                }
                slot => break slot,
            }
        };

        let new_entry = entry_factory();
        *slot = Some(new_entry);
        self.growth_left -= 1;

        new_entry
    }

    pub(crate) fn allocated_memory(&self) -> usize {
        if self.allocated() {
            mem::size_of::<Option<Entry>>() * self.capacity()
        } else {
            0
        }
    }
}

impl Entries {
    /// where the Entries has allocated memory
    #[inline]
    pub(crate) fn allocated(&self) -> bool {
        self.mask != 0
    }

    #[inline]
    pub(crate) fn capacity(&self) -> usize {
        self.mask + 1
    }
}

impl Entries {
    unsafe fn grow(&mut self) {
        let cur_capacity = self.capacity();

        let new_capacity = Self::next_capacity(cur_capacity);
        let new_mask = Self::capacity_to_mask(new_capacity);

        let new_data = {
            let layout = Self::layout_of_capacity(new_capacity);
            let allocated = alloc(layout);
            if allocated.is_null() {
                panic!("oom")
            }
            NonNull::new_unchecked(allocated as *mut Option<Entry>)
        };

        // zeroed
        ptr::write_bytes(new_data.as_ptr(), 0, new_capacity);

        let cur_items_count = Self::max_item_count(cur_capacity);

        {
            let mut remaining_items_count = cur_items_count;

            let cur_entry_slice = slice::from_raw_parts(self.data.as_ptr(), cur_capacity);

            for e in cur_entry_slice {
                match e {
                    None => continue,
                    Some(entry) => {
                        let hash = entry.hash();
                        let mut pos = (hash as usize) & new_mask;
                        let mut dist = 0;
                        let slot = loop {
                            let slot = &mut *new_data.as_ptr().add(pos);
                            if slot.is_none() {
                                break slot;
                            }

                            dist += 1;
                            pos = pos.wrapping_add(dist) & new_mask;
                        };

                        *slot = Some(*entry);
                        remaining_items_count -= 1;
                        if remaining_items_count == 0 {
                            break;
                        }
                    }
                }
            }
        }

        // dealloc current data
        self.try_dealloc_data();

        self.data = new_data;
        self.growth_left = Self::max_item_count(new_capacity) - cur_items_count;
        self.mask = new_mask;
    }

    unsafe fn try_dealloc_data(&self) {
        if self.allocated() {
            dealloc(
                self.data.as_ptr() as *mut u8,
                Self::layout_of_capacity(self.capacity()),
            );
        }
    }
}

impl Entries {
    #[inline]
    const fn capacity_to_mask(capacity: usize) -> usize {
        capacity - 1
    }

    #[inline]
    const fn next_capacity(capacity: usize) -> usize {
        // every newly created Entries has a capacity of 1
        if capacity == 1 {
            ENTRIES_INITIAL_CAPACITY
        } else {
            capacity * 2
        }
    }

    #[inline]
    const fn max_item_count(capacity: usize) -> usize {
        capacity / 4 * 3
    }

    #[inline]
    const fn layout_of_capacity(capacity: usize) -> Layout {
        let size = mem::size_of::<Option<Entry>>() * capacity;
        unsafe { Layout::from_size_align_unchecked(size, ALLOC_ALIGNMENT) }
    }
}

impl Drop for Entries {
    fn drop(&mut self) {
        unsafe { self.try_dealloc_data() }
    }
}

impl Default for Entries {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for Entries {}
