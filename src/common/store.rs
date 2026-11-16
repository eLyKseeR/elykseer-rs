use std::sync::Arc;

use crate::assembly::{BlockInformation, KeyInformation};
use crate::common::configuration::Configuration;
use crate::common::filesupport::FileInformation;

// ---------------------------------------------------------------------------
// Generic store
// ---------------------------------------------------------------------------

/// Coq: Record store (KVs : Type)
#[derive(Clone)]
pub struct Store<V: Clone> {
    pub sconfig: Arc<Configuration>,
    pub entries: Vec<(String, V)>,
}

impl<V: Clone> Store<V> {
    pub fn init(c: Arc<Configuration>) -> Self {
        Store {
            sconfig: c,
            entries: Vec::new(),
        }
    }

    pub fn add(&self, k: &str, v: V) -> Self {
        let mut new_entries = self.entries.clone();
        new_entries.push((k.to_string(), v));
        Store {
            sconfig: Arc::clone(&self.sconfig),
            entries: new_entries,
        }
    }

    /// Coq: rec_find — the entry added last wins (Coq prepends new entries
    /// and returns the first match).
    pub fn find(&self, k: &str) -> Option<&V> {
        self.entries
            .iter()
            .rev()
            .find(|(key, _)| key == k)
            .map(|(_, val)| val)
    }

    /// Coq: rec_find_all — find all matching entries
    pub fn find_all(&self, k: &str) -> Vec<&V> {
        let mut result = Vec::new();
        for (key, val) in &self.entries {
            if key == k {
                result.push(val);
            }
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Type aliases for the three store types
// ---------------------------------------------------------------------------

/// Coq: KeyListStore — maps aid_t → keyinformation
pub type KeyListStore = Store<KeyInformation>;

/// Coq: FBlockListStore — maps string (fhash) → blockinformation
pub type FBlockListStore = Store<BlockInformation>;

/// Coq: FileinformationStore — maps string (fname) → fileinformation
pub type FileinformationStore = Store<FileInformation>;
