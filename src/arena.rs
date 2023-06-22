use crate::constants::{ALLOC_ALIGNMENT, CHUNK_DEFAULT_CAPACITY, CHUNK_USABLE_THRESHOLD};
use std::alloc::{alloc, dealloc, Layout};
use std::cell::Cell;
use std::mem::size_of;
use std::ptr::{copy_nonoverlapping, eq, write, NonNull};

pub(crate) struct Arena {
    chunk: Cell<NonNull<Chunk>>,
}

impl Arena {
    pub(crate) fn new() -> Self {
        Self {
            chunk: Cell::new(DUMMY_CHUNK.get()),
        }
    }

    pub(crate) fn alloc_str(&mut self, hash: u64, string: &str) -> NonNull<u8> {
        let str_len = string.len();
        let char_ptr = string.as_ptr();
        if let Some(ptr) = unsafe { self.try_alloc_str_fast_path(hash, str_len, char_ptr) } {
            ptr
        } else {
            unsafe { self.try_alloc_str_slow_path(hash, str_len, char_ptr) }
        }
    }

    pub(crate) fn allocated_memory(&self) -> usize {
        let mut size = 0;
        let mut chunk = self.chunk.get();
        unsafe {
            while !chunk.as_ref().is_dummy() {
                size += chunk.as_ref().size;
                chunk = chunk.as_ref().prev;
            }
        };
        size
    }
}

impl Arena {
    #[inline]
    unsafe fn try_alloc_str_fast_path(
        &mut self,
        hash: u64,
        str_len: usize,
        char_ptr: *const u8,
    ) -> Option<NonNull<u8>> {
        self.chunk
            .get()
            .as_ref()
            .try_alloc_str(hash, str_len, char_ptr)
    }

    unsafe fn try_alloc_str_slow_path(
        &mut self,
        hash: u64,
        str_len: usize,
        char_ptr: *const u8,
    ) -> NonNull<u8> {
        let cur_chunk = self.chunk.get();

        let needed_bytes = Chunk::needed_bytes_for_string(str_len).expect("too large");
        let new_chunk = if Chunk::is_exceed_default_capacity(needed_bytes) {
            let chunk = Chunk::new_for_needed_bytes(cur_chunk, needed_bytes);
            // after create a Chunk for a large string, should check if the prev one
            // is still usable
            if cur_chunk.as_ref().is_still_usable() {
                Chunk::swap(cur_chunk, chunk);
            } else {
                self.chunk.set(chunk);
            }
            chunk
        } else {
            let chunk = Chunk::new(cur_chunk);
            self.chunk.set(chunk);
            chunk
        };

        new_chunk
            .as_ref()
            .try_alloc_str(hash, str_len, char_ptr)
            .expect("internal error")
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        let mut chunk = self.chunk.get();
        unsafe {
            while !chunk.as_ref().is_dummy() {
                let layout =
                    Layout::from_size_align_unchecked(chunk.as_ref().size, ALLOC_ALIGNMENT);
                let prev = chunk.as_ref().prev;
                dealloc(chunk.as_ref().low, layout);
                chunk = prev;
            }
        }
    }
}

unsafe impl Send for Arena {}
struct Chunk {
    prev: NonNull<Chunk>,
    size: usize,
    /// points to the start of the last allocated string
    cur: Cell<*mut u8>,
    /// the start of the allocated chunk
    low: *mut u8,
}

impl Chunk {
    pub(crate) fn new(prev: NonNull<Chunk>) -> NonNull<Self> {
        unsafe { Self::try_new_with_size(prev, CHUNK_DEFAULT_CAPACITY).expect("oom") }
    }

    pub(crate) fn new_for_needed_bytes(prev: NonNull<Chunk>, bytes: usize) -> NonNull<Self> {
        unsafe {
            Self::try_new_with_size(
                prev,
                bytes.checked_add(size_of::<Chunk>()).expect("too large"),
            )
            .expect("oom")
        }
    }
}

impl Chunk {
    pub(crate) const fn is_exceed_default_capacity(needed_bytes: usize) -> bool {
        needed_bytes > CHUNK_DEFAULT_CAPACITY - size_of::<Chunk>()
    }

    pub(crate) const fn needed_bytes_for_string(str_len: usize) -> Option<usize> {
        // len + hash + chars
        str_len.checked_add(size_of::<usize>() + size_of::<u64>())
    }

    pub(crate) fn swap(mut first: NonNull<Chunk>, mut second: NonNull<Chunk>) {
        unsafe {
            second.as_mut().prev = first.as_ref().prev;
            first.as_mut().prev = second
        }
    }
}

impl Chunk {
    pub(crate) fn is_dummy(&self) -> bool {
        eq(self, DUMMY_CHUNK.get().as_ptr())
    }

    pub(crate) fn is_still_usable(&self) -> bool {
        (self.cur.get() as usize - self.low as usize) >= CHUNK_USABLE_THRESHOLD
    }

    #[inline]
    unsafe fn try_new_with_size(prev: NonNull<Chunk>, size: usize) -> Option<NonNull<Self>> {
        let size = round_up(size, ALLOC_ALIGNMENT).expect("too large");

        let layout = Layout::from_size_align_unchecked(size, ALLOC_ALIGNMENT);

        let low = alloc(layout);
        if low.is_null() {
            None
        } else {
            // every chunk holds itself in the tail of allocated memory so we can operate
            // pointers of chunks instead of values
            let high = low.add(size) as *mut Chunk;
            let chunk_self_start = high.sub(1);
            write(
                chunk_self_start,
                Chunk {
                    prev,
                    size,
                    cur: Cell::new(chunk_self_start as *mut u8),
                    low,
                },
            );
            Some(NonNull::new_unchecked(chunk_self_start))
        }
    }

    unsafe fn try_alloc_str(
        &self,
        hash: u64,
        str_len: usize,
        src_char_ptr: *const u8,
    ) -> Option<NonNull<u8>> {
        let cur = self.cur.get() as usize;
        let dest_char_ptr = round_down(
            cur.checked_sub(str_len).expect("too large"),
            ALLOC_ALIGNMENT,
        );
        if dest_char_ptr < self.low as usize + size_of::<usize>() + size_of::<u64>() {
            None
        } else {
            // copy chars
            copy_nonoverlapping(src_char_ptr, dest_char_ptr as *mut u8, str_len);

            // write length
            let dest_len_start = (dest_char_ptr as *mut usize).sub(1);
            write(dest_len_start, str_len);

            // write hash
            let dest_hash_start = (dest_len_start as *mut u64).sub(1);
            write(dest_hash_start, hash);

            self.cur.set(dest_hash_start as *mut u8);

            Some(NonNull::new_unchecked(dest_len_start as *mut u8))
        }
    }
}

#[repr(transparent)]
struct DummyChunk(Chunk);

impl DummyChunk {
    fn get(&'static self) -> NonNull<Chunk> {
        unsafe { NonNull::new_unchecked(&self.0 as *const Chunk as *mut Chunk) }
    }
}

unsafe impl Sync for DummyChunk {}

static DUMMY_CHUNK: DummyChunk = DummyChunk(Chunk {
    prev: NonNull::dangling(),
    size: 0,
    cur: Cell::new(&DUMMY_CHUNK as *const DummyChunk as *mut u8),
    low: &DUMMY_CHUNK as *const DummyChunk as *mut u8,
});

#[inline]
const fn round_up(n: usize, alignment: usize) -> Option<usize> {
    debug_assert!(alignment > 0);
    debug_assert!(alignment.is_power_of_two());
    if let Some(added) = n.checked_add(alignment - 1) {
        Some(added & !(alignment - 1))
    } else {
        None
    }
}

#[inline]
const fn round_down(n: usize, alignment: usize) -> usize {
    debug_assert!(alignment > 0);
    debug_assert!(alignment.is_power_of_two());
    n & !(alignment - 1)
}
