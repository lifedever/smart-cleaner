use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use trash::delete;
use std::time::SystemTime;

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
    pub min_size_mb: Option<u64>,     // x MB
    pub created_before_ms: Option<u64>, // timestamp
    pub modified_before_ms: Option<u64>, // timestamp
    pub extensions: Option<Vec<String>>,
    pub include_empty_dirs: Option<bool>,
}

#[tauri::command]
async fn scan_directory(options: ScanOptions) -> Result<ScanResult, String> {
    let target_path = Path::new(&options.target_dir);
    if !target_path.exists() || !target_path.is_dir() {
        return Err("Invalid or non-existent target directory".into());
    }

    let min_size_bytes = options.min_size_mb.map(|mb| mb * 1024 * 1024);
    let mut files = Vec::new();
    let mut total_size = 0;

    let walker = WalkDir::new(target_path).into_iter();

    for entry in walker.filter_entry(|e| e.path() != target_path) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // skip errors (e.g. permission denied)
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_symlink() {
            continue;
        }

        let is_dir = metadata.is_dir();
        let size = if is_dir { 0 } else { metadata.len() };
        
        let mut should_include = true;

        if !is_dir {
            // size filter
            if let Some(min_size) = min_size_bytes {
                if size < min_size {
                    should_include = false;
                }
            }

            // extension filter
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

        let created_at = metadata.created().ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
            
        let modified_at = metadata.modified().ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);

         // Time filter
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

        // Empty dir logic
        if is_dir && should_include {
            if options.include_empty_dirs.unwrap_or(false) {
                let is_empty = fs::read_dir(path).map(|mut d| d.next().is_none()).unwrap_or(false);
                if !is_empty {
                    should_include = false;
                }
            } else {
                should_include = false; // By default don't delete directories directly
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

    Ok(ScanResult { files, total_size })
}

#[tauri::command]
async fn move_to_trash(paths: Vec<String>) -> Result<(), String> {
    for path_str in paths {
        let path = PathBuf::from(&path_str);
        if path.exists() {
            if let Err(e) = delete(&path) {
                // Log the error but try continuing with other files
                eprintln!("Failed to delete {}: {}", path_str, e);
            }
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![scan_directory, move_to_trash])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
