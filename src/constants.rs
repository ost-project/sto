use std::mem::size_of;

pub(crate) const ALLOC_ALIGNMENT: usize = size_of::<usize>();

/// 8 KiB
pub(crate) const CHUNK_DEFAULT_CAPACITY: usize = 1 << 13;

/// 128 B
pub(crate) const CHUNK_USABLE_THRESHOLD: usize = 1 << 7;

pub(crate) const BUCKET_MASK_BITS: usize = 6;

pub(crate) const BUCKET_NUMBER: usize = 1 << BUCKET_MASK_BITS;

pub(crate) const BUCKET_RSHIFT: usize = usize::BITS as usize - BUCKET_MASK_BITS;

/// 64 bit: 1024 * 8 B = 8 KiB
/// 32 bit: 1024 * 4 B = 4 KiB
pub(crate) const ENTRIES_INITIAL_CAPACITY: usize = 1 << 10;
