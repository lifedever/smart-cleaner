use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub files: Vec<FileInfo>,
    pub total_size: u64,
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
    pub min_size_mb: Option<u64>,        // x MB
    pub created_before_ms: Option<u64>,  // timestamp
    pub modified_before_ms: Option<u64>, // timestamp
    pub extensions: Option<Vec<String>>,
    pub include_empty_dirs: Option<bool>,
    pub whitelist: Option<Vec<String>>,
}

use tauri::{AppHandle, Emitter};

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
async fn scan_directory(app: AppHandle, options: ScanOptions) -> Result<ScanResult, String> {
    let target_path = PathBuf::from(&options.target_dir);
    if !target_path.exists() || !target_path.is_dir() {
        return Err("Invalid or non-existent target directory".into());
    }

    let min_size_bytes = options.min_size_mb.map(|mb| mb * 1024 * 1024);

    let whitelist: std::collections::HashSet<String> =
        options.whitelist.unwrap_or_default().into_iter().collect();

    // Process on a separate thread to unblock Tauri/Vue entirely
    let (files, total_size) = tauri::async_runtime::spawn_blocking(move || {
        let mut files = Vec::new();
        let mut total_size = 0;
        let mut scanned_count = 0;

        let walker = WalkDir::new(&target_path)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| {
                let path_str = e.path().to_string_lossy();
                !whitelist.contains(path_str.as_ref())
            });

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            scanned_count += 1;

            if scanned_count % 50 == 0 {
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress {
                        scanned_count,
                        matched_count: files.len() as u64,
                        current_path: path.to_string_lossy().to_string(),
                    },
                );
            }

            let metadata = match fs::symlink_metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if metadata.is_symlink() {
                continue;
            }

            let is_dir = metadata.is_dir();
            let size = if is_dir { 0 } else { metadata.len() };
            let file_name = entry.file_name().to_string_lossy();

            // Ignore .DS_Store and Thumbs.db entirely in the list
            let lower_name = file_name.to_lowercase();
            if lower_name == ".ds_store" || lower_name == "thumbs.db" {
                continue;
            }

            let mut should_include = true;

            if !is_dir {
                if let Some(min_size) = min_size_bytes {
                    if size < min_size {
                        should_include = false;
                    }
                }

                if should_include {
                    if let Some(exts) = &options.extensions {
                        if !exts.is_empty() {
                            let file_ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                            if !exts.iter().any(|ext| ext.eq_ignore_ascii_case(file_ext)) {
                                should_include = false;
                            }
                        }
                    }
                }
            }

            let created_at = metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);

            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);

            if should_include && !is_dir {
                if let Some(created_before) = options.created_before_ms {
                    if let Some(created) = created_at {
                        if created >= created_before {
                            should_include = false;
                        }
                    }
                }
                if let Some(modified_before) = options.modified_before_ms {
                    if let Some(modified) = modified_at {
                        if modified >= modified_before {
                            should_include = false;
                        }
                    }
                }
            }

            if is_dir && should_include {
                if options.include_empty_dirs.unwrap_or(false) {
                    let is_empty = fs::read_dir(path)
                        .map(|entries| {
                            entries.filter_map(|e| e.ok()).all(|e| {
                                let name = e.file_name();
                                let name_str = name.to_string_lossy();
                                // Consider dir empty if it only contains .DS_Store or is truly empty
                                name_str == ".DS_Store" || name_str == "Thumbs.db"
                            })
                        })
                        .unwrap_or(false);
                    if !is_empty {
                        should_include = false;
                    }
                } else {
                    should_include = false;
                }
            }

            if should_include {
                let id = uuid::Uuid::new_v4().to_string();
                files.push(FileInfo {
                    id,
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: path.to_string_lossy().to_string(),
                    size,
                    created_at,
                    modified_at,
                    is_dir,
                });
                total_size += size;
            }
        }

        // Final progress ping inside thread
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                scanned_count,
                matched_count: files.len() as u64,
                current_path: "文件收集完毕，正在渲染...".to_string(),
            },
        );

        (files, total_size)
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(ScanResult { files, total_size })
}

#[tauri::command]
async fn move_to_trash(
    app: AppHandle,
    paths: Vec<String>,
    target_dir: String,
) -> Result<(), String> {
    let mut errors = Vec::new();
    let total = paths.len() as u64;
    let mut current = 0;
    let target_path = PathBuf::from(&target_dir);

    for path_str in paths {
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
            let res = trash::delete(&path);

            if let Err(e) = res {
                let err_msg = format!("Failed to delete {}: {}", path_str, e);
                eprintln!("{}", err_msg);
                errors.push(err_msg);
            } else {
                // If it's a file, we might want to clean up its parent folder if it became empty
                remove_parent_if_empty(&path, &target_path);
            }
        }
    }
    if !errors.is_empty() {
        return Err(format!("部分文件未能删除:\n{}", errors.join("\n")));
    }
    Ok(())
}

fn remove_parent_if_empty(path: &PathBuf, target_dir: &PathBuf) {
    if let Some(parent) = path.parent() {
        let parent_ptr = parent.to_path_buf();
        // Only clean up parents INSIDE the target directory
        if parent_ptr == *target_dir || !parent_ptr.starts_with(target_dir) {
            return;
        }

        let is_empty = fs::read_dir(&parent_ptr)
            .map(|entries| {
                entries.filter_map(|e| e.ok()).all(|e| {
                    let name = e.file_name();
                    let name_str = name.to_string_lossy().to_lowercase();
                    name_str == ".ds_store" || name_str == "thumbs.db"
                })
            })
            .unwrap_or(false);

        if is_empty {
            if let Ok(_) = fs::remove_dir_all(&parent_ptr) {
                // Recurse to see if the grand-parent is now also empty
                remove_parent_if_empty(&parent_ptr, target_dir);
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![scan_directory, move_to_trash])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
