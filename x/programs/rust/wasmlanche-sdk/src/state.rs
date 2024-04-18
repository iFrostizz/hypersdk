use crate::memory::into_bytes;
use crate::program::Program;
use crate::state::Error as StateError;
use borsh::{from_slice, to_vec, BorshDeserialize, BorshSerialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;

#[derive(Clone, thiserror::Error, Debug)]
pub enum Error {
    #[error("an unclassified error has occurred: {0}")]
    Other(String),

    #[error("invalid byte format")]
    InvalidBytes,

    #[error("invalid byte length: {0}")]
    InvalidByteLength(usize),

    #[error("invalid tag: {0}")]
    InvalidTag(u8),

    #[error("failed to write to host storage")]
    Write,

    #[error("failed to read from host storage")]
    Read,

    #[error("failed to serialize bytes")]
    Serialization,

    #[error("failed to deserialize bytes")]
    Deserialization,

    #[error("failed to convert integer")]
    IntegerConversion,

    #[error("failed to delete from host storage")]
    Delete,
}

struct Cache<K>
where
    K: Into<Key> + Hash + PartialEq + Eq + Clone,
{
    reads: HashMap<K, Vec<u8>>,
    writes: HashMap<K, Vec<u8>>,
    flushed: bool,
}

pub struct State<K>
where
    K: Into<Key> + Hash + PartialEq + Eq + Clone,
{
    program: Program,
    cache: Cache<K>,
}

impl<K> Drop for State<K>
where
    K: Into<Key> + Hash + PartialEq + Eq + Clone,
{
    fn drop(&mut self) {
        if !self.cache.flushed {
            self.flush().unwrap();
        }
    }
}

impl<K> State<K>
where
    K: Into<Key> + Hash + PartialEq + Eq + Clone,
{
    #[must_use]
    pub fn new(program: Program) -> Self {
        Self {
            program,
            cache: Cache {
                reads: HashMap::new(),
                writes: HashMap::new(),
                flushed: false,
            },
        }
    }

    /// Store a key and value to the host storage. If the key already exists,
    /// the value will be overwritten.
    /// # Errors
    /// Returns an [Error] if the key or value cannot be
    /// serialized or if the host fails to handle the operation.
    pub fn store<V>(&mut self, key: K, value: V) -> Result<(), Error>
    where
        V: BorshSerialize,
    {
        let serialized = to_vec(&value).map_err(|_| StateError::Deserialization)?;
        self.cache.writes.insert(key, serialized);

        Ok(())
    }

    /// Get a value from the host's storage.
    ///
    /// Note: The pointer passed to the host are only valid for the duration of this
    /// function call. This function will take ownership of the pointer and free it.
    ///
    /// # Errors
    /// Returns an [Error] if the key cannot be serialized or if
    /// the host fails to read the key and value.
    /// # Panics
    /// Panics if the value cannot be converted from i32 to usize.
    pub fn get<V>(&mut self, key: K) -> Result<V, Error>
    where
        V: BorshDeserialize,
    {
        let val_bytes = if let Some(val) = self.cache.reads.get(&key) {
            val.clone()
        } else {
            let val_ptr = unsafe { host::get_bytes(&self.program, &key.clone().into())? };
            if val_ptr < 0 {
                return Err(Error::Read);
            }

            // Wrap in OK for now, change from_raw_ptr to return Result
            into_bytes(val_ptr)
        };

        self.cache.reads.insert(key, val_bytes.clone());

        from_slice::<V>(&val_bytes).map_err(|_| StateError::Deserialization)
    }

    /// Delete a value from the hosts's storage.
    /// # Errors
    /// Returns an [Error] if the key cannot be serialized
    /// or if the host fails to delete the key and the associated value
    pub fn delete(&mut self, key: K) -> Result<(), Error> {
        self.cache.writes.remove(&key);
        self.cache.reads.remove(&key);

        unsafe { host::delete_bytes(&self.program, &key.into()) }
    }

    /// Apply all pending operations to storage and mark the cache as flushed
    pub fn flush(&mut self) -> Result<(), Error> {
        for (key, value) in self.cache.writes.drain() {
            unsafe {
                host::put_bytes(&self.program, &key.into(), &value)?;
            }
        }

        self.cache.flushed = true;

        Ok(())
    }
}

/// Key is a wrapper around a `Vec<u8>` that represents a key in the host storage.
#[derive(Debug, Default, Clone)]
pub struct Key(Vec<u8>);

impl Deref for Key {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Key {
    /// Returns a new Key from the bytes.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

mod host {
    use super::{BorshSerialize, Key, Program};
    use crate::{memory::to_host_ptr, state::Error};

    #[link(wasm_import_module = "state")]
    extern "C" {
        #[link_name = "put"]
        fn _put(caller: i64, key: i64, value: i64) -> i64;

        #[link_name = "get"]
        fn _get(caller: i64, key: i64) -> i64;

        #[link_name = "delete"]
        fn _delete(caller: i64, key: i64) -> i64;
    }

    /// Persists the bytes at `value` at key on the host storage.
    pub(super) unsafe fn put_bytes<V>(caller: &Program, key: &Key, value: &V) -> Result<(), Error>
    where
        V: BorshSerialize,
    {
        let value_bytes = borsh::to_vec(value).map_err(|_| Error::Serialization)?;
        // prepend length to both key & value
        let caller = to_host_ptr(caller.id())?;
        let value = to_host_ptr(&value_bytes)?;
        let key = to_host_ptr(key)?;

        match unsafe { _put(caller, key, value) } {
            0 => Ok(()),
            _ => Err(Error::Write),
        }
    }

    /// Gets the bytes associated with the key from the host.
    pub(super) unsafe fn get_bytes(caller: &Program, key: &Key) -> Result<i64, Error> {
        // prepend length to key
        let caller = to_host_ptr(caller.id())?;
        let key = to_host_ptr(key)?;
        Ok(unsafe { _get(caller, key) })
    }

    /// Deletes the bytes at key ptr from the host storage
    pub(super) unsafe fn delete_bytes(caller: &Program, key: &Key) -> Result<(), Error> {
        let caller = to_host_ptr(caller.id())?;
        let key = to_host_ptr(key)?;
        match unsafe { _delete(caller, key) } {
            0 => Ok(()),
            _ => Err(Error::Delete),
        }
    }
}
