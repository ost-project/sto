//! # Sto
//! `sto` is a string interning crate, just like `string-interner`, `lasso` and `ustr`,
//! but maintains lifetime-bounded, pointer-sized interned strings,
//! and allocates memory only when needed.
//!
//! - [Repository], a thread-safe struct where strings are stored,
//! - [ScopedSto], a handle to access the interned string,
//! - [Sto], an alias of `ScopedSto<'static>`,
//! - to intern a string, see [ScopedSto::intern_in],
//! - to get the interned string, see [ScopedSto::as_str],
//! - to check memory footprint, see [Repository::allocated_memory],
//! - to access the global Repository provided by feature `global`, see [repository()],
//! - to intern a string in the global Repository, see [Sto::from].
//!
//! ## Features
//!
//! | Name   | Default | Description                               |
//! |--------|---------|-------------------------------------------|
//! | global | ✅       | provide a shared global Repository        |
#![deny(missing_debug_implementations, unreachable_pub, missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;

mod arena;
mod constants;
mod entry;
mod repository;

use crate::entry::Entry;
pub use crate::repository::Repository;

/// Represents an interned string.
///
/// ## Size
/// It has the same size as a pointer.
///
/// ## Intern
/// See [ScopedSto::intern_in] and [ScopedSto::from].
///
/// ## Clone, Hash
/// It can be cheaply cloned, copied, and used to calculate hash because it stores the hash of string.
/// ```
/// # use sto::{ScopedSto, Repository};
/// let repository = Repository::new();
/// let a = ScopedSto::intern_in("hello", &repository);
/// let b = a.clone();
/// let hash = a.hash();
/// ```
///
/// ## Compare
/// Two `ScopedSto`s can be compared cheaply **only when they are stored in the same `Repository`**.
/// To compare two `ScopedSto` in different `Repository`, convert them to `&str` first.
/// ```
/// # use sto::{ScopedSto, Repository};
/// let repository_a = Repository::new();
/// let a = ScopedSto::intern_in("hello", &repository_a);
/// // compare with strings directly
/// assert_eq!(a, "hello");
/// {
///   let b = ScopedSto::intern_in("hello", &repository_a);
///   // compared cheaply
///   assert_eq!(a, b)
/// }
///
/// {
///   let repository_b = Repository::new();
///   let b = ScopedSto::intern_in("hello", &repository_b);
///   // cannot compared directly
///   assert_ne!(a, b);
///   // convert to &str first
///   assert_eq!(a.as_str(), b.as_str());
/// }
/// ```
///
/// ## Lifetime
/// The lifetime of `ScopedSto` is tied to the [Repository] that actually stores data.
/// That means when the `Repository` dropped, the `ScopedSto`s from it would be invalid.
///
/// A `'static` `Repository` would create `'static` `ScopedSto`s, which is very common because
/// programs usually use a global `'static` `Repository`, so for convenience,
/// `ScopedSto<'static>` has an alias named [Sto].
///
/// ## Thread Safe
/// `ScopedSto`s can be shared between threads safely.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ScopedSto<'a> {
    pub(crate) entry: Entry,
    _phantom: PhantomData<&'a ()>,
}

/// Alias for `'static` [ScopedSto].
pub type Sto = ScopedSto<'static>;

impl Hash for ScopedSto<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash().hash(state)
    }
}

impl<'a> ScopedSto<'a> {
    fn new(entry: Entry) -> Self {
        Self {
            entry,
            _phantom: Default::default(),
        }
    }

    /// The interned string.
    pub fn as_str(&self) -> &'a str {
        self.entry.as_str()
    }

    /// The precomputed hash.
    pub fn hash(&self) -> u64 {
        self.entry.hash()
    }

    /// The length of the interned string.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.as_str().len()
    }
}

impl<'a> ScopedSto<'a> {
    /// Intern a string in the given [Repository].
    #[inline(always)]
    pub fn intern_in<S>(string: S, repository: &'a Repository) -> Self
    where
        S: AsRef<str>,
    {
        Self::new(repository.get_or_insert(string.as_ref()))
    }
}

unsafe impl Send for ScopedSto<'_> {}

unsafe impl Sync for ScopedSto<'_> {}

impl PartialEq<&str> for ScopedSto<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for ScopedSto<'_> {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<'a> AsRef<str> for ScopedSto<'a> {
    fn as_ref(&self) -> &'a str {
        self.as_str()
    }
}

impl<'a> From<ScopedSto<'a>> for &'a str {
    fn from(value: ScopedSto<'a>) -> Self {
        value.as_str()
    }
}

impl PartialOrd<Self> for ScopedSto<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl Ord for ScopedSto<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<'a> Deref for ScopedSto<'a> {
    type Target = str;

    fn deref(&self) -> &'a Self::Target {
        self.as_str()
    }
}

impl fmt::Display for ScopedSto<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for ScopedSto<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returns a reference to the default global shared [Repository].
///
/// [ScopedSto::from] is a shortcut to intern a string in this `Repository`.
#[cfg(feature = "global")]
#[cfg_attr(docsrs, doc(cfg(feature = "global")))]
pub fn repository() -> &'static Repository {
    use once_cell::sync::OnceCell;
    static REPO: OnceCell<Repository> = OnceCell::new();

    REPO.get_or_init(Repository::new)
}

#[cfg(feature = "global")]
impl ScopedSto<'static> {
    /// A shortcut to intern a string in the default global shared [Repository].
    ///
    /// Returns a 'static [ScopedSto].
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "global")]
    /// # use sto::{repository, Sto};
    /// # fn main() {
    ///     let interned_str = Sto::from("example string");
    ///     // equals to
    ///     {
    ///       let repo = repository();
    ///       let interned_str = Sto::intern_in("example string", &repo);
    ///     }
    /// # }
    /// ```
    #[cfg_attr(docsrs, doc(cfg(feature = "global")))]
    #[inline(always)]
    pub fn from<S>(string: S) -> Self
    where
        S: AsRef<str>,
    {
        Self::intern_in(string, repository())
    }
}

#[cfg(feature = "global")]
#[cfg_attr(docsrs, doc(cfg(feature = "global")))]
impl From<&str> for ScopedSto<'static> {
    fn from(value: &str) -> Self {
        Self::from(value)
    }
}

#[cfg(feature = "global")]
#[cfg_attr(docsrs, doc(cfg(feature = "global")))]
impl From<String> for ScopedSto<'static> {
    fn from(value: String) -> Self {
        Self::from(value)
    }
}

#[cfg(feature = "global")]
#[cfg_attr(docsrs, doc(cfg(feature = "global")))]
impl From<std::borrow::Cow<'_, str>> for ScopedSto<'static> {
    fn from(value: std::borrow::Cow<'_, str>) -> Self {
        Self::from(value)
    }
}

#[cfg(feature = "global")]
#[cfg_attr(docsrs, doc(cfg(feature = "global")))]
impl std::str::FromStr for ScopedSto<'static> {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::constants::{CHUNK_DEFAULT_CAPACITY, ENTRIES_INITIAL_CAPACITY};
    use crate::{Repository, ScopedSto};
    use std::mem::size_of;

    #[test]
    fn test_scoped_sto() {
        let repo = Repository::new();
        {
            let a = ScopedSto::intern_in("hello world", &repo);
            let b = ScopedSto::intern_in("hello world", &repo);
            assert_eq!(a, b);
            assert_eq!(a.as_str(), b.as_str());
            assert_eq!(a.hash(), b.hash());
        }

        {
            let a = ScopedSto::intern_in("hello", &repo);
            let b = ScopedSto::intern_in("world", &repo);
            assert_ne!(a, b);
            assert_ne!(a.as_str(), b.as_str());
            assert_ne!(a.hash(), b.hash());
        }

        {
            let large_string = "test".repeat(CHUNK_DEFAULT_CAPACITY);
            let a = ScopedSto::intern_in(&large_string, &repo);
            assert_eq!(a, large_string);
        }
    }

    #[test]
    fn test_allocated_memory() {
        let repo = Repository::new();
        ScopedSto::intern_in("hello world", &repo);
        ScopedSto::intern_in("hello", &repo);
        ScopedSto::intern_in("world", &repo);

        assert_eq!(
            repo.allocated_memory(),
            3 * (CHUNK_DEFAULT_CAPACITY + size_of::<usize>() * ENTRIES_INITIAL_CAPACITY)
        )
    }

    #[test]
    #[cfg(feature = "global")]
    fn test_sto() {
        use crate::Sto;

        let a = Sto::from("hello world");
        let b = Sto::from("hello world");
        assert_eq!(a, b);
        assert_eq!(a.as_str(), b.as_str());
        assert_eq!(a.hash(), b.hash());

        let a = Sto::from("hello");
        let b = Sto::from("world");
        assert_ne!(a, b);
        assert_ne!(a.as_str(), b.as_str());
        assert_ne!(a.hash(), b.hash());
    }
}
