// ported from foundry's executor with some modifications
// https://github.com/foundry-rs/foundry/blob/master/evm/src/executor/fork/database.rs
use super::{
    blockchain_db::BlockchainDb, errors::DatabaseError, shared_backend::SharedBackend,
    snapshot::StateSnapshot,
};
use ethers::{prelude::U256, types::BlockId};
use hashbrown::HashMap as Map;
use log::{trace, warn};
use parking_lot::Mutex;
use revm::db::CacheDB;
use revm::{
    db::DatabaseRef,
    primitives::{Account, AccountInfo, Bytecode, B160, B256, U256 as rU256},
    Database, DatabaseCommit,
};
use std::sync::Arc;

/// a [revm::Database] that's forked off another client
///
/// The `backend` is used to retrieve (missing) data, which is then fetched from the remote
/// endpoint. The inner in-memory database holds this storage and will be used for write operations.
/// This database uses the `backend` for read and the `db` for write operations. But note the
/// `backend` will also write (missing) data to the `db` in the background
#[derive(Debug, Clone)]
pub struct ForkedDatabase {
    /// responsible for fetching missing data
    ///
    /// This is responsible for getting data
    backend: SharedBackend,
    /// Cached Database layer, ensures that changes are not written to the database that
    /// exclusively stores the state of the remote client.
    ///
    /// This separates Read/Write operations
    ///   - reads from the `SharedBackend as DatabaseRef` writes to the internal cache storage
    cache_db: CacheDB<SharedBackend>,
    /// Contains all the data already fetched
    ///
    /// This exclusively stores the _unchanged_ remote client state
    db: BlockchainDb,
    /// holds the snapshot state of a blockchain
    snapshots: Arc<Mutex<Snapshots<ForkDbSnapshot>>>,
}

impl ForkedDatabase {
    /// Creates a new instance of this DB
    pub fn new(backend: SharedBackend, db: BlockchainDb) -> Self {
        Self {
            cache_db: CacheDB::new(backend.clone()),
            backend,
            db,
            snapshots: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn database(&self) -> &CacheDB<SharedBackend> {
        &self.cache_db
    }

    pub fn database_mut(&mut self) -> &mut CacheDB<SharedBackend> {
        &mut self.cache_db
    }

    pub fn snapshots(&self) -> &Arc<Mutex<Snapshots<ForkDbSnapshot>>> {
        &self.snapshots
    }

    /// Reset the fork to a fresh forked state, and optionally update the fork config
    pub fn reset(
        &mut self,
        _url: Option<String>,
        block_number: impl Into<BlockId>,
    ) -> Result<(), String> {
        self.backend
            .set_pinned_block(block_number)
            .map_err(|err| err.to_string())?;

        // TODO need to find a way to update generic provider via url

        // wipe the storage retrieved from remote
        self.inner().db().clear();
        // create a fresh `CacheDB`, effectively wiping modified state
        self.cache_db = CacheDB::new(self.backend.clone());
        trace!(target: "backend::forkdb", "Cleared database");
        Ok(())
    }

    /// Flushes the cache to disk if configured
    pub fn flush_cache(&self) {
        self.db.cache().flush()
    }

    /// Returns the database that holds the remote state
    pub fn inner(&self) -> &BlockchainDb {
        &self.db
    }

    pub fn create_snapshot(&self) -> ForkDbSnapshot {
        let db = self.db.db();
        let snapshot = StateSnapshot {
            accounts: db.accounts.read().clone(),
            storage: db.storage.read().clone(),
            block_hashes: db.block_hashes.read().clone(),
        };
        ForkDbSnapshot {
            local: self.cache_db.clone(),
            snapshot,
        }
    }

    pub fn insert_snapshot(&self) -> U256 {
        let snapshot = self.create_snapshot();
        let mut snapshots = self.snapshots().lock();
        let id = snapshots.insert(snapshot);
        trace!(target: "backend::forkdb", "Created new snapshot {}", id);
        id
    }

    pub fn revert_snapshot(&mut self, id: U256) -> bool {
        let snapshot = { self.snapshots().lock().remove(id) };
        if let Some(snapshot) = snapshot {
            let ForkDbSnapshot {
                local,
                snapshot:
                    StateSnapshot {
                        accounts,
                        storage,
                        block_hashes,
                    },
            } = snapshot;
            let db = self.inner().db();
            {
                let mut accounts_lock = db.accounts.write();
                accounts_lock.clear();
                accounts_lock.extend(accounts);
            }
            {
                let mut storage_lock = db.storage.write();
                storage_lock.clear();
                storage_lock.extend(storage);
            }
            {
                let mut block_hashes_lock = db.block_hashes.write();
                block_hashes_lock.clear();
                block_hashes_lock.extend(block_hashes);
            }

            self.cache_db = local;

            trace!(target: "backend::forkdb", "Reverted snapshot {}", id);
            true
        } else {
            warn!(target: "backend::forkdb", "No snapshot to revert for {}", id);
            false
        }
    }
}

impl Database for ForkedDatabase {
    type Error = DatabaseError;

    fn basic(&mut self, address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        // Note: this will always return Some, since the `SharedBackend` will always load the
        // account, this differs from `<CacheDB as Database>::basic`, See also
        // [MemDb::ensure_loaded](crate::executor::backend::MemDb::ensure_loaded)
        Database::basic(&mut self.cache_db, address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        Database::code_by_hash(&mut self.cache_db, code_hash)
    }

    fn storage(&mut self, address: B160, index: rU256) -> Result<rU256, Self::Error> {
        Database::storage(&mut self.cache_db, address, index)
    }

    fn block_hash(&mut self, number: rU256) -> Result<B256, Self::Error> {
        Database::block_hash(&mut self.cache_db, number)
    }
}

impl DatabaseRef for ForkedDatabase {
    type Error = DatabaseError;

    fn basic(&self, address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        self.cache_db.basic(address)
    }

    fn code_by_hash(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.cache_db.code_by_hash(code_hash)
    }

    fn storage(&self, address: B160, index: rU256) -> Result<rU256, Self::Error> {
        DatabaseRef::storage(&self.cache_db, address, index)
    }

    fn block_hash(&self, number: rU256) -> Result<B256, Self::Error> {
        self.cache_db.block_hash(number)
    }
}

impl DatabaseCommit for ForkedDatabase {
    fn commit(&mut self, changes: Map<B160, Account>) {
        self.database_mut().commit(changes)
    }
}

/// Represents a snapshot of the database
///
/// This mimics `revm::CacheDB`
#[derive(Debug)]
pub struct ForkDbSnapshot {
    pub local: CacheDB<SharedBackend>,
    pub snapshot: StateSnapshot,
}

// === impl DbSnapshot ===

impl ForkDbSnapshot {
    fn get_storage(&self, address: B160, index: rU256) -> Option<rU256> {
        self.local
            .accounts
            .get(&address)
            .and_then(|account| account.storage.get(&index))
            .copied()
    }
}

// This `DatabaseRef` implementation works similar to `CacheDB` which prioritizes modified elements,
// and uses another db as fallback
// We prioritize stored changed accounts/storage
impl DatabaseRef for ForkDbSnapshot {
    type Error = DatabaseError;

    fn basic(&self, address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        match self.local.accounts.get(&address) {
            Some(account) => Ok(Some(account.info.clone())),
            None => {
                let mut acc = self.snapshot.accounts.get(&address).cloned();

                if acc.is_none() {
                    acc = self.local.basic(address)?;
                }
                Ok(acc)
            }
        }
    }

    fn code_by_hash(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.local.code_by_hash(code_hash)
    }

    fn storage(&self, address: B160, index: rU256) -> Result<rU256, Self::Error> {
        match self.local.accounts.get(&address) {
            Some(account) => match account.storage.get(&index) {
                Some(entry) => Ok(*entry),
                None => match self.get_storage(address, index) {
                    None => DatabaseRef::storage(&self.local, address, index),
                    Some(storage) => Ok(storage),
                },
            },
            None => match self.get_storage(address, index) {
                None => DatabaseRef::storage(&self.local, address, index),
                Some(storage) => Ok(storage),
            },
        }
    }

    fn block_hash(&self, number: rU256) -> Result<B256, Self::Error> {
        match self.snapshot.block_hashes.get(&number).copied() {
            None => self.local.block_hash(number),
            Some(block_hash) => Ok(block_hash),
        }
    }
}

/// Represents all snapshots
#[derive(Debug, Clone)]
pub struct Snapshots<T> {
    id: U256,
    snapshots: Map<U256, T>,
}

// === impl Snapshots ===

impl<T> Snapshots<T> {
    fn next_id(&mut self) -> U256 {
        let id = self.id;
        self.id = id.saturating_add(U256::one());
        id
    }

    /// Returns the snapshot with the given id `id`
    pub fn get(&self, id: U256) -> Option<&T> {
        self.snapshots.get(&id)
    }

    /// Removes the snapshot with the given `id`.
    ///
    /// This will also remove any snapshots taken after the snapshot with the `id`. e.g.: reverting
    /// to id 1 will delete snapshots with ids 1, 2, 3, etc.)
    pub fn remove(&mut self, id: U256) -> Option<T> {
        let snapshot = self.snapshots.remove(&id);

        // revert all snapshots taken after the snapshot
        let mut to_revert = id + 1;
        while to_revert < self.id {
            self.snapshots.remove(&to_revert);
            to_revert = to_revert + 1;
        }

        snapshot
    }

    /// Inserts the new snapshot and returns the id
    pub fn insert(&mut self, snapshot: T) -> U256 {
        let id = self.next_id();
        self.snapshots.insert(id, snapshot);
        id
    }
}

impl<T> Default for Snapshots<T> {
    fn default() -> Self {
        Self {
            id: U256::zero(),
            snapshots: Map::new(),
        }
    }
}