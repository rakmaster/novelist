//! Recent-projects list, persisted as JSON in the platform config dir.

use std::path::PathBuf;

const MAX_RECENTS: usize = 10;

fn recents_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Novelist").join("recents.json"))
}

pub fn load() -> Vec<PathBuf> {
    let Some(path) = recents_path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    serde_json::from_str::<Vec<PathBuf>>(&text).unwrap_or_default()
}

pub fn save(paths: &[PathBuf]) {
    let Some(path) = recents_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(paths) {
        let _ = std::fs::write(&path, json);
    }
}

/// Add `path` to the front of the list, deduplicated, capped at MAX_RECENTS.
pub fn push(paths: &mut Vec<PathBuf>, path: PathBuf) {
    paths.retain(|p| p != &path);
    paths.insert(0, path);
    paths.truncate(MAX_RECENTS);
    save(paths);
}

/// Remove `path` from the list and persist.
pub fn remove(paths: &mut Vec<PathBuf>, path: &PathBuf) {
    paths.retain(|p| p != path);
    save(paths);
}
