use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use tauri::{AppHandle, Emitter};

pub struct AppState {
    pub cancel_scan: Arc<AtomicBool>,
    pub cancel_clean: AtomicBool,
}

#[tauri::command]
fn cancel_scan(state: tauri::State<'_, AppState>) {
    state.cancel_scan.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn cancel_clean(state: tauri::State<'_, AppState>) {
    state.cancel_clean.store(true, Ordering::Relaxed);
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    pub permission_errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub created_at: Option<u64>,
    pub modified_at: Option<u64>,
    pub is_dir: bool,
}

#[derive(Debug, Deserialize)]
pub struct ScanOptions {
    pub target_dir: String,
    pub min_size_mb: Option<u64>,
    pub max_size_mb: Option<u64>,
    pub created_before_ms: Option<u64>,
    pub modified_before_ms: Option<u64>,
    pub extensions: Option<Vec<String>>,
    pub include_empty_dirs: Option<bool>,
    pub include_hidden: Option<bool>,
    pub whitelist: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
struct ScanProgress {
    scanned_count: u64,
    matched_count: u64,
    current_path: String,
}

#[derive(Clone, Serialize)]
struct CleanProgress {
    total: u64,
    current: u64,
    current_path: String,
}

#[tauri::command]
async fn scan_directory(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    options: ScanOptions,
) -> Result<ScanResult, String> {
    state.cancel_scan.store(false, Ordering::Relaxed);
    let cancel_scan = state.cancel_scan.clone();
    let target_path = PathBuf::from(&options.target_dir);
    if !target_path.exists() || !target_path.is_dir() {
        return Err("Invalid or non-existent target directory".into());
    }

    let min_size_bytes = options.min_size_mb.map(|mb| mb * 1024 * 1024);
    let max_size_bytes = options.max_size_mb.map(|mb| mb * 1024 * 1024);
    let include_hidden = options.include_hidden.unwrap_or(false);
    let include_empty_dirs = options.include_empty_dirs.unwrap_or(false);
    let has_date_filter =
        options.created_before_ms.is_some() || options.modified_before_ms.is_some();

    let whitelist: HashSet<String> =
        options.whitelist.unwrap_or_default().into_iter().collect();

    // Pre-lowercase extension patterns (avoid per-file allocation)
    let ext_patterns: Option<Vec<String>> = options.extensions.map(|exts| {
        exts.into_iter()
            .filter(|e| !e.is_empty())
            .map(|e| e.to_lowercase())
            .collect()
    });

    let (files, total_size, permission_errors) = tauri::async_runtime::spawn_blocking(move || {
        let mut files = Vec::new();
        let mut total_size: u64 = 0;
        let scanned_count = AtomicU64::new(0);
        let matched_count = AtomicU64::new(0);
        let mut permission_errors: Vec<String> = Vec::new();
        let mut id_counter: u64 = 0;

        // jwalk: parallel directory walker, multiple threads read dirs concurrently
        let walker = jwalk::WalkDir::new(&target_path)
            .min_depth(1)
            .process_read_dir(move |_depth, _path, _state, children| {
                // Filter entries in-place (runs in worker threads)
                children.retain(|entry_result| {
                    if let Ok(entry) = entry_result {
                        let name = entry.file_name.to_string_lossy();
                        // Skip hidden files/dirs
                        if !include_hidden && name.starts_with('.') {
                            return false;
                        }
                        // Whitelist check
                        let p = entry.path();
                        let path_str = p.to_string_lossy();
                        if whitelist.contains(path_str.as_ref()) {
                            return false;
                        }
                    }
                    true
                });
            });

        for entry in walker {
            // Check cancel flag
            if cancel_scan.load(Ordering::Relaxed) {
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    // Collect permission errors
                    if let Some(io_err) = err.io_error() {
                        if io_err.kind() == std::io::ErrorKind::PermissionDenied {
                            if let Some(ancestor) = err.path() {
                                permission_errors
                                    .push(ancestor.to_string_lossy().to_string());
                            }
                        }
                    }
                    continue;
                }
            };

            let count = scanned_count.fetch_add(1, Ordering::Relaxed);
            // Emit progress every 500 entries (reduce IPC overhead)
            if count % 500 == 0 {
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress {
                        scanned_count: count,
                        matched_count: matched_count.load(Ordering::Relaxed),
                        current_path: entry.path().to_string_lossy().to_string(),
                    },
                );
            }

            // Use file_type from readdir (free, no extra syscall)
            let file_type = match entry.file_type {
                ft => ft,
            };
            if file_type.is_symlink() {
                continue;
            }

            let is_dir = file_type.is_dir();
            let path = entry.path();
            let file_name = entry.file_name.to_string_lossy();

            // Skip system junk files
            let lower_name = file_name.to_lowercase();
            if lower_name == ".ds_store" || lower_name == "thumbs.db" {
                continue;
            }

            // --- Directory handling ---
            if is_dir {
                if !include_empty_dirs {
                    continue;
                }
                let is_empty = fs::read_dir(&path)
                    .map(|entries| {
                        entries.filter_map(|e| e.ok()).all(|e| {
                            let n = e.file_name();
                            let s = n.to_string_lossy();
                            s == ".DS_Store" || s == "Thumbs.db"
                        })
                    })
                    .unwrap_or(false);
                if !is_empty {
                    continue;
                }

                id_counter += 1;
                files.push(FileInfo {
                    id: id_counter.to_string(),
                    name: file_name.to_string(),
                    path: path.to_string_lossy().to_string(),
                    size: 0,
                    created_at: None,
                    modified_at: None,
                    is_dir: true,
                });
                matched_count.fetch_add(1, Ordering::Relaxed);
                continue;
            }

            // --- File handling: get metadata only when needed ---
            let metadata = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        permission_errors.push(path.to_string_lossy().to_string());
                    }
                    continue;
                }
            };

            let size = metadata.len();

            // Size filters (cheap, check first)
            if let Some(min_size) = min_size_bytes {
                if size < min_size {
                    continue;
                }
            }
            if let Some(max_size) = max_size_bytes {
                if size > max_size {
                    continue;
                }
            }

            // Extension filter (pre-lowercased patterns)
            if let Some(ref patterns) = ext_patterns {
                if !patterns.is_empty() {
                    let name_lower = lower_name; // already lowercased above
                    let matched = patterns.iter().any(|pat| {
                        if pat.starts_with('.') {
                            name_lower.ends_with(pat.as_str())
                        } else {
                            name_lower.contains(pat.as_str())
                        }
                    });
                    if !matched {
                        continue;
                    }
                }
            }

            // Date filters (only compute timestamps when filters exist)
            let (created_at, modified_at) = if has_date_filter {
                let c = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64);
                let m = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64);

                if let Some(created_before) = options.created_before_ms {
                    if let Some(created) = c {
                        if created >= created_before {
                            continue;
                        }
                    }
                }
                if let Some(modified_before) = options.modified_before_ms {
                    if let Some(modified) = m {
                        if modified >= modified_before {
                            continue;
                        }
                    }
                }
                (c, m)
            } else {
                // Still provide timestamps for UI display (sort by date)
                let c = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64);
                let m = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64);
                (c, m)
            };

            // Passed all filters — include this file
            id_counter += 1;
            files.push(FileInfo {
                id: id_counter.to_string(),
                name: file_name.to_string(),
                path: path.to_string_lossy().to_string(),
                size,
                created_at,
                modified_at,
                is_dir: false,
            });
            total_size += size;
            matched_count.fetch_add(1, Ordering::Relaxed);
        }

        // Final progress
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                scanned_count: scanned_count.load(Ordering::Relaxed),
                matched_count: matched_count.load(Ordering::Relaxed),
                current_path: "文件收集完毕，正在渲染...".to_string(),
            },
        );

        (files, total_size, permission_errors)
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(ScanResult {
        files,
        total_size,
        permission_errors,
    })
}

/// 将待删除的文件列表按父目录合并：
/// 如果某个目录下的所有有效文件都被选中删除，则直接删除整个目录（保留目录结构进回收站），
/// 而不是逐个删除文件再清理空目录。与 Finder 删除行为一致。
fn consolidate_to_parent_dirs(paths: &[String], target_dir: &PathBuf) -> Vec<String> {
    let path_set: HashSet<PathBuf> = paths.iter().map(PathBuf::from).collect();

    // 收集非根目录的唯一父目录
    let mut parent_dirs: HashSet<PathBuf> = HashSet::new();
    for p in &path_set {
        if let Some(parent) = p.parent() {
            if parent != target_dir.as_path() {
                parent_dirs.insert(parent.to_path_buf());
            }
        }
    }

    // 检查哪些父目录可以整体删除
    let mut consolidated_parents: HashSet<PathBuf> = HashSet::new();
    for parent in &parent_dirs {
        if let Ok(entries) = fs::read_dir(parent) {
            let can_consolidate = entries.filter_map(|e| e.ok()).all(|entry| {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // 无关紧要的系统文件，或已在删除列表中
                name_str == ".DS_Store"
                    || name_str == "Thumbs.db"
                    || path_set.contains(&entry.path())
            });
            if can_consolidate {
                consolidated_parents.insert(parent.clone());
            }
        }
    }

    // 构建最终删除列表：被合并目录覆盖的文件跳过，改为删除目录本身
    let mut result: Vec<String> = Vec::new();
    for path_str in paths {
        let path = PathBuf::from(path_str);
        if let Some(parent) = path.parent() {
            if consolidated_parents.contains(&parent.to_path_buf()) {
                continue;
            }
        }
        result.push(path_str.clone());
    }
    for dir in &consolidated_parents {
        result.push(dir.to_string_lossy().to_string());
    }

    result
}

#[tauri::command]
async fn move_to_trash(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
    target_dir: String,
) -> Result<(), String> {
    state.cancel_clean.store(false, Ordering::Relaxed);
    let mut errors = Vec::new();

    let mut context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }

    let target_path = PathBuf::from(&target_dir);
    // 合并：如果某目录下所有文件都被选中，则整体删除该目录
    let final_paths = consolidate_to_parent_dirs(&paths, &target_path);
    let total = final_paths.len() as u64;
    let mut current = 0;

    for path_str in final_paths {
        if state.cancel_clean.load(Ordering::Relaxed) {
            break;
        }
        current += 1;
        let path = PathBuf::from(&path_str);

        // Emit progress
        let _ = app.emit(
            "clean-progress",
            CleanProgress {
                total,
                current,
                current_path: path_str.clone(),
            },
        );

        if path.exists() {
            let res = context.delete(&path);

            if let Err(e) = res {
                let err_msg = format!("Failed to delete {}: {}", path_str, e);
                eprintln!("{}", err_msg);
                errors.push(err_msg);
            }
        }
    }

    if !errors.is_empty() {
        return Err(format!("部分文件未能删除:\n{}", errors.join("\n")));
    }
    Ok(())
}

#[tauri::command]
fn show_in_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let parent = std::path::Path::new(&path)
            .parent()
            .unwrap_or(std::path::Path::new(""));
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn is_directory(path: String) -> bool {
    std::path::Path::new(&path).is_dir()
}

#[tauri::command]
fn open_trash() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"Finder\"\nopen trash\nactivate\nend tell")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("shell:RecycleBinFolder")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct DiskUsage {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

#[tauri::command]
fn get_disk_usage(path: String) -> Result<DiskUsage, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("路径不存在".into());
    }
    let total = fs2::total_space(p).map_err(|e| e.to_string())?;
    let available = fs2::available_space(p).map_err(|e| e.to_string())?;
    let used = total.saturating_sub(available);
    Ok(DiskUsage {
        total_bytes: total,
        available_bytes: available,
        used_bytes: used,
    })
}

#[tauri::command]
fn export_whitelist(path: String, whitelist: Vec<String>) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&whitelist).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn import_whitelist(path: String) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let list: Vec<String> =
        serde_json::from_str(&content).map_err(|e| format!("无效的白名单文件格式: {}", e))?;
    Ok(list)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            cancel_scan: Arc::new(AtomicBool::new(false)),
            cancel_clean: AtomicBool::new(false),
        })
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan_directory,
            cancel_scan,
            move_to_trash,
            cancel_clean,
            show_in_folder,
            is_directory,
            open_trash,
            get_disk_usage,
            export_whitelist,
            import_whitelist,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
