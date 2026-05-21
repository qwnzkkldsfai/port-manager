mod aggregator;
mod declared;
mod excluded;
mod listening;
mod llm;
mod scanner;
mod static_ports;

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{Manager, State};

use aggregator::{FreeSegment, PortStatus, Snapshot};
use declared::{DeclaredEntry, DeclaredStore, StoreState};
use excluded::ExcludedRange;
use listening::ListeningEntry;
use llm::{LlmConfig, LlmRefineResult};
use scanner::ScanCandidate;
use serde::Deserialize;

#[derive(Deserialize)]
struct PendingEntry {
    port: u16,
    label: String,
    source_file: String,
    line: u32,
    context: String,
}

#[derive(Default)]
struct Cache {
    listening: Vec<ListeningEntry>,
    excluded: Vec<ExcludedRange>,
    refreshed_at: i64,
}

struct AppState {
    store: StoreState,
    cache: Mutex<Cache>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Serialize)]
struct RefreshSnapshot {
    listening: Vec<ListeningEntry>,
    excluded: Vec<ExcludedRange>,
    declared: DeclaredStore,
    refreshed_at: i64,
    is_admin: bool,
}

fn map_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
fn refresh(state: State<'_, AppState>) -> Result<RefreshSnapshot, String> {
    let listening = listening::enumerate().map_err(map_err)?;
    let excluded = excluded::enumerate().map_err(map_err)?;
    let ts = now_secs();
    {
        let mut c = state.cache.lock().unwrap();
        c.listening = listening.clone();
        c.excluded = excluded.clone();
        c.refreshed_at = ts;
    }
    let declared = state.store.snapshot();
    Ok(RefreshSnapshot {
        listening,
        excluded,
        declared,
        refreshed_at: ts,
        is_admin: is_elevated::is_elevated(),
    })
}

#[tauri::command]
fn get_declared(state: State<'_, AppState>) -> DeclaredStore {
    state.store.snapshot()
}

#[tauri::command]
fn query_port(port: u16, state: State<'_, AppState>) -> PortStatus {
    let cache = state.cache.lock().unwrap();
    let declared_snapshot = state.store.snapshot();
    let snap = Snapshot {
        listening: &cache.listening,
        excluded: &cache.excluded,
        declared_entries: &declared_snapshot.entries,
    };
    aggregator::status_for_port(port, &snap)
}

#[tauri::command]
fn scan_directory(path: String) -> Result<Vec<ScanCandidate>, String> {
    scanner::scan_path(&path).map_err(map_err)
}

#[tauri::command]
fn commit_entries(
    entries: Vec<PendingEntry>,
    group_label: String,
    source_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<DeclaredStore, String> {
    let ts = now_secs();
    let fallback = if group_label.trim().is_empty() {
        if let Some(sp) = source_path.as_ref() {
            std::path::Path::new(sp)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名".to_string())
        } else {
            "未命名".to_string()
        }
    } else {
        group_label
    };
    let entries: Vec<DeclaredEntry> = entries
        .into_iter()
        .map(|e| {
            let label = if e.label.trim().is_empty() {
                fallback.clone()
            } else {
                e.label
            };
            DeclaredEntry {
                id: 0,
                port: e.port,
                label,
                source_file: e.source_file,
                line: e.line,
                context: e.context,
                added_at: ts,
            }
        })
        .collect();
    state.store.add_entries(entries);
    if let Some(sp) = source_path {
        state.store.mark_scanned(&sp, ts);
    }
    state.store.persist().map_err(map_err)?;
    Ok(state.store.snapshot())
}

#[tauri::command]
fn add_scan_path(path: String, label: String, state: State<'_, AppState>) -> Result<DeclaredStore, String> {
    let label = if label.trim().is_empty() {
        std::path::Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone())
    } else {
        label
    };
    state.store.upsert_scan_path(path, label);
    state.store.persist().map_err(map_err)?;
    Ok(state.store.snapshot())
}

#[tauri::command]
fn remove_scan_path(path: String, state: State<'_, AppState>) -> Result<DeclaredStore, String> {
    state.store.remove_scan_path(&path);
    state.store.persist().map_err(map_err)?;
    Ok(state.store.snapshot())
}

#[tauri::command]
fn delete_declared(id: u64, state: State<'_, AppState>) -> Result<DeclaredStore, String> {
    state.store.remove_entry(id);
    state.store.persist().map_err(map_err)?;
    Ok(state.store.snapshot())
}

#[tauri::command]
fn clear_declared(state: State<'_, AppState>) -> Result<(DeclaredStore, usize), String> {
    let n = state.store.clear_entries();
    state.store.persist().map_err(map_err)?;
    Ok((state.store.snapshot(), n))
}

#[tauri::command]
fn update_declared_label(id: u64, label: String, state: State<'_, AppState>) -> Result<DeclaredStore, String> {
    state.store.update_entry_label(id, label);
    state.store.persist().map_err(map_err)?;
    Ok(state.store.snapshot())
}

#[tauri::command]
fn free_segments(
    range_start: u16,
    range_end: u16,
    min_length: u32,
    state: State<'_, AppState>,
) -> Vec<FreeSegment> {
    let cache = state.cache.lock().unwrap();
    let occ = aggregator::occupied_set(&cache.listening, &cache.excluded, &state.store);
    aggregator::all_free_segments(&occ, range_start, range_end, min_length.max(1))
}

#[tauri::command]
fn is_admin() -> bool {
    is_elevated::is_elevated()
}

#[tauri::command]
fn restart_as_admin(app: tauri::AppHandle) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().to_string();
    let escaped = exe_str.replace('\'', "''");
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!("Start-Process -Verb RunAs -FilePath '{}'", escaped),
        ])
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn get_llm_config(state: State<'_, AppState>) -> LlmConfig {
    state.store.llm_config()
}

#[tauri::command]
fn set_llm_config(cfg: LlmConfig, state: State<'_, AppState>) -> Result<LlmConfig, String> {
    state.store.set_llm_config(cfg);
    state.store.persist().map_err(map_err)?;
    Ok(state.store.llm_config())
}

#[tauri::command]
async fn llm_health(cfg: LlmConfig) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || llm::check_health(&cfg))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn llm_refine(
    candidates: Vec<ScanCandidate>,
    state: State<'_, AppState>,
) -> Result<LlmRefineResult, String> {
    let cfg = state.store.llm_config();
    tauri::async_runtime::spawn_blocking(move || llm::refine_all(&cfg, candidates))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|fp| match fp {
                tauri_plugin_dialog::FilePath::Path(p) => Some(p.to_string_lossy().to_string()),
                _ => None,
            })
    })
    .await
    .ok()
    .flatten()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = declared::default_store_path();
            let store = StoreState::load(path);
            app.manage(AppState {
                store,
                cache: Mutex::new(Cache::default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            refresh,
            get_declared,
            query_port,
            scan_directory,
            commit_entries,
            add_scan_path,
            remove_scan_path,
            delete_declared,
            clear_declared,
            update_declared_label,
            free_segments,
            is_admin,
            pick_folder,
            restart_as_admin,
            get_llm_config,
            set_llm_config,
            llm_health,
            llm_refine,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
