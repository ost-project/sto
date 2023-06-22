use crate::arena::Arena;
use crate::constants::{BUCKET_NUMBER, BUCKET_RSHIFT};
use crate::entry::{Entries, Entry};
use ahash::RandomState;
use parking_lot::Mutex;
use std::fmt;
use std::fmt::Formatter;
use std::hash::{BuildHasher, Hasher};

/// A [Repository] used to store interned strings.
///
/// Once a `Repository` is dropped,
/// all interned strings held by it become unavailable.
/// This is typically ensured by Rust's lifetime mechanism.
///
/// The `Repository` can be safely shared among multiple threads.
///
/// To intern a string, see [ScopedSto::intern_in](crate::ScopedSto::intern_in).
pub struct Repository {
    buckets: [Bucket; BUCKET_NUMBER],
}

impl Repository {
    /// Constructs a new [Repository].
    ///
    /// The newly constructed `Repository` does not allocate memory initially.
    /// Memory allocation only occurs when there is a string to be interned.
    ///
    ///
    ///
    /// ## Example
    /// ```
    /// # use sto::Repository;
    /// let repository = Repository::new();
    /// ```
    pub fn new() -> Self {
        Self {
            buckets: [(); BUCKET_NUMBER].map(|_| Bucket::default()),
        }
    }

    /// Returns the number of bytes allocated by the [Repository].
    ///
    /// ## Example
    /// ```
    /// # use sto::{Repository, ScopedSto};
    /// let repository = Repository::new();
    /// ScopedSto::intern_in("hello", &repository);
    ///
    /// let allocated_memory = repository.allocated_memory();
    /// println!("Allocated memory: {} bytes", allocated_memory);
    /// ```
    pub fn allocated_memory(&self) -> usize {
        self.buckets
            .iter()
            .enumerate()
            .map(|(_, b)| {
                let b = b.0.lock();
                b.entries.allocated_memory() + b.arena.allocated_memory()
            })
            .sum()
    }
}

impl Repository {
    pub(crate) fn get_or_insert(&self, string: &str) -> Entry {
        let hash = Self::get_hash(string);
        self.buckets[Self::determine_bucket(hash)]
            .0
            .lock()
            .get_or_insert(hash, string)
    }
}

impl Repository {
    fn get_hash(string: &str) -> u64 {
        static RANDOM: RandomState =
            RandomState::with_seeds(0x01230456, 0x04560789, 0x07890123, 0x02580137);
        let mut hasher = RANDOM.build_hasher();
        hasher.write(string.as_bytes());
        hasher.finish()
    }

    const fn determine_bucket(hash: u64) -> usize {
        (hash >> BUCKET_RSHIFT) as usize
    }
}

impl Default for Repository {
    /// See [Repository::new].
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Repository {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Repository").finish()
    }
}

#[repr(align(32))]
#[derive(Default)]
struct Bucket(Mutex<BucketImpl>);

/// BucketImpl has 32 bytes on 64 bit hardware
#[derive(Default)]
struct BucketImpl {
    arena: Arena,
    entries: Entries,
}

impl BucketImpl {
    #[inline]
    fn get_or_insert(&mut self, hash: u64, string: &str) -> Entry {
        self.entries
            .get_or_insert(hash, string, || Entry(self.arena.alloc_str(hash, string)))
    }
}
