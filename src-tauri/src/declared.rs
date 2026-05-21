use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::llm::LlmConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclaredEntry {
    pub id: u64,
    pub port: u16,
    pub label: String,
    pub source_file: String,
    pub line: u32,
    pub context: String,
    pub added_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DeclaredStore {
    pub next_id: u64,
    pub entries: Vec<DeclaredEntry>,
    pub scan_paths: Vec<ScanPath>,
    #[serde(default)]
    pub llm_config: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanPath {
    pub path: String,
    pub label: String,
    pub last_scanned: Option<i64>,
}

pub struct StoreState {
    pub path: PathBuf,
    pub inner: Mutex<DeclaredStore>,
}

impl StoreState {
    pub fn load(path: PathBuf) -> Self {
        let inner = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(txt) => serde_json::from_str(&txt).unwrap_or_default(),
                Err(_) => DeclaredStore::default(),
            }
        } else {
            DeclaredStore::default()
        };
        StoreState {
            path,
            inner: Mutex::new(inner),
        }
    }

    pub fn persist(&self) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap();
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let txt = serde_json::to_string_pretty(&*guard)?;
        fs::write(&self.path, txt)?;
        Ok(())
    }

    pub fn snapshot(&self) -> DeclaredStore {
        let guard = self.inner.lock().unwrap();
        DeclaredStore {
            next_id: guard.next_id,
            entries: guard.entries.clone(),
            scan_paths: guard.scan_paths.clone(),
            llm_config: guard.llm_config.clone(),
        }
    }

    pub fn llm_config(&self) -> LlmConfig {
        self.inner.lock().unwrap().llm_config.clone()
    }

    pub fn set_llm_config(&self, cfg: LlmConfig) {
        self.inner.lock().unwrap().llm_config = cfg;
    }

    pub fn add_entries(&self, items: Vec<DeclaredEntry>) {
        let mut guard = self.inner.lock().unwrap();
        for mut e in items {
            guard.next_id += 1;
            e.id = guard.next_id;
            guard.entries.push(e);
        }
    }

    pub fn remove_entry(&self, id: u64) {
        let mut guard = self.inner.lock().unwrap();
        guard.entries.retain(|e| e.id != id);
    }

    pub fn clear_entries(&self) -> usize {
        let mut guard = self.inner.lock().unwrap();
        let n = guard.entries.len();
        guard.entries.clear();
        n
    }

    pub fn update_entry_label(&self, id: u64, label: String) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(e) = guard.entries.iter_mut().find(|e| e.id == id) {
            e.label = label;
        }
    }

    pub fn upsert_scan_path(&self, path: String, label: String) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(sp) = guard.scan_paths.iter_mut().find(|p| p.path == path) {
            sp.label = label;
        } else {
            guard.scan_paths.push(ScanPath {
                path,
                label,
                last_scanned: None,
            });
        }
    }

    pub fn remove_scan_path(&self, path: &str) {
        let mut guard = self.inner.lock().unwrap();
        guard.scan_paths.retain(|p| p.path != path);
    }

    pub fn mark_scanned(&self, path: &str, when: i64) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(sp) = guard.scan_paths.iter_mut().find(|p| p.path == path) {
            sp.last_scanned = Some(when);
        }
    }

    pub fn all_declared_ports(&self) -> Vec<u16> {
        let guard = self.inner.lock().unwrap();
        let mut s: Vec<u16> = guard.entries.iter().map(|e| e.port).collect();
        s.sort_unstable();
        s.dedup();
        s
    }
}

pub fn default_store_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("PortManager").join("declared.json")
}
