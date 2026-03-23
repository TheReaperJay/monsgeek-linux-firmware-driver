use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type DbKey = (String, Vec<u8>);

#[derive(Debug, Clone, Default)]
pub struct DbStore {
    inner: Arc<Mutex<HashMap<DbKey, Vec<u8>>>>,
}

impl DbStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, db_path: &str, key: &[u8]) -> Vec<u8> {
        let store = self.inner.lock().expect("db store poisoned");
        store
            .get(&(db_path.to_string(), key.to_vec()))
            .cloned()
            .unwrap_or_default()
    }

    pub fn insert(&self, db_path: String, key: Vec<u8>, value: Vec<u8>) {
        let mut store = self.inner.lock().expect("db store poisoned");
        store.insert((db_path, key), value);
    }

    pub fn delete(&self, db_path: &str, key: &[u8]) {
        let mut store = self.inner.lock().expect("db store poisoned");
        store.remove(&(db_path.to_string(), key.to_vec()));
    }

    pub fn all_keys(&self, db_path: &str) -> Vec<Vec<u8>> {
        let store = self.inner.lock().expect("db store poisoned");
        store
            .keys()
            .filter(|(path, _)| path == db_path)
            .map(|(_, key)| key.clone())
            .collect()
    }

    pub fn all_values(&self, db_path: &str) -> Vec<Vec<u8>> {
        let store = self.inner.lock().expect("db store poisoned");
        store
            .iter()
            .filter(|((path, _), _)| path == db_path)
            .map(|(_, value)| value.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::DbStore;

    #[test]
    fn db_insert_get_roundtrip() {
        let db = DbStore::new();
        db.insert("settings".to_string(), b"key".to_vec(), b"value".to_vec());
        assert_eq!(db.get("settings", b"key"), b"value".to_vec());
    }
}
