pub mod core;
mod db;

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::{Emitter, Manager};

const MAX_PERSISTED_RETRY_ATTEMPTS: i64 = 3;
const PROCESS_PROGRESS_EVENT: &str = "process-progress";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportJobState {
    status: String,
    total_files: i64,
    downloaded_files: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportMemoriesResult {
    parsed_count: usize,
    imported_count: usize,
    skipped_duplicates: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessMemoriesResult {
    processed_count: usize,
    failed_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessProgressPayload {
    total_files: usize,
    completed_files: usize,
    successful_files: usize,
    failed_files: usize,
    memory_item_id: Option<i64>,
    status: String,
    error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailItem {
    memory_item_id: i64,
    thumbnail_path: String,
}

fn memories_db_url(app: &tauri::AppHandle) -> Result<String, String> {
    let mut app_config_dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("failed to resolve app config directory: {error}"))?;

    std::fs::create_dir_all(&app_config_dir)
        .map_err(|error| format!("failed to create app config directory: {error}"))?;

    app_config_dir.push("memories.db");

    Ok(format!("sqlite://{}", app_config_dir.to_string_lossy()))
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn get_job_state(app: tauri::AppHandle) -> Result<ExportJobState, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let row = sqlx::query(
        "SELECT status, total_files, downloaded_files FROM ExportJob ORDER BY id ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("failed to read export job state: {error}"))?;

    pool.close().await;

    if let Some(row) = row {
        Ok(ExportJobState {
            status: row.get::<String, _>("status"),
            total_files: row.get::<i64, _>("total_files"),
            downloaded_files: row.get::<i64, _>("downloaded_files"),
        })
    } else {
        Ok(ExportJobState {
            status: "idle".to_string(),
            total_files: 0,
            downloaded_files: 0,
        })
    }
}

#[tauri::command]
async fn set_job_state(app: tauri::AppHandle, state: ExportJobState) -> Result<(), String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    sqlx::query(
        "
        INSERT INTO ExportJob (id, status, total_files, downloaded_files)
        VALUES (1, ?1, ?2, ?3)
        ON CONFLICT(id) DO UPDATE SET
            status = excluded.status,
            total_files = excluded.total_files,
            downloaded_files = excluded.downloaded_files
        ",
    )
    .bind(state.status)
    .bind(state.total_files)
    .bind(state.downloaded_files)
    .execute(&pool)
    .await
    .map_err(|error| format!("failed to write export job state: {error}"))?;

    pool.close().await;
    Ok(())
}

#[tauri::command]
async fn download_queued_memories(
    app: tauri::AppHandle,
    window: tauri::Window,
    output_dir: String,
    requests_per_minute: Option<u32>,
    concurrent_downloads: Option<u32>,
) -> Result<usize, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let rows = sqlx::query(
        "
        SELECT id, media_url, overlay_url, retry_count
        FROM MemoryItem
        WHERE status IN ('queued', 'retryable')
        ORDER BY id ASC
        ",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load resumable memories: {error}"))?;

    let total_files = i64::try_from(rows.len())
        .map_err(|error| format!("failed to convert total resumable file count: {error}"))?;

    sqlx::query(
        "
        INSERT INTO ExportJob (id, status, total_files, downloaded_files)
        VALUES (1, 'running', ?1, 0)
        ON CONFLICT(id) DO UPDATE SET
            status = 'running',
            total_files = excluded.total_files,
            downloaded_files = 0
        ",
    )
    .bind(total_files)
    .execute(&pool)
    .await
    .map_err(|error| format!("failed to initialize running export job state: {error}"))?;

    let mut retry_counts_by_id = std::collections::HashMap::new();
    let mut overlay_urls_by_id = std::collections::HashMap::new();

    let tasks = rows
        .iter()
        .map(|row| {
            let id = row.get::<i64, _>("id");
            let media_url = row.get::<String, _>("media_url");
            let overlay_url = row.get::<Option<String>, _>("overlay_url");
            let retry_count = row.get::<i64, _>("retry_count");

            retry_counts_by_id.insert(id, retry_count);
            overlay_urls_by_id.insert(id, overlay_url);

            let extension = extract_extension_from_url(&media_url, "bin");

            core::downloader::DownloadTask {
                memory_item_id: id,
                url: media_url,
                destination_path: std::path::Path::new(&output_dir)
                    .join(format!("{id}.{extension}")),
            }
        })
        .collect::<Vec<_>>();

    let rate_limits = core::downloader::DownloadRateLimits {
        requests_per_minute: requests_per_minute
            .map(|value| value as usize)
            .unwrap_or(core::downloader::DEFAULT_REQUESTS_PER_MINUTE),
        concurrent_downloads: concurrent_downloads
            .map(|value| value as usize)
            .unwrap_or(core::downloader::MAX_CONCURRENT_DOWNLOADS),
    };

    let download_results = core::downloader::download_tasks_with_progress_and_rate_limits(
        &window,
        tasks,
        rate_limits,
    )
    .await
    .map_err(|error| format!("download manager failed: {error}"))?;

    let mut successful_downloads = 0usize;
    let mut successful_media_ids = Vec::new();

    for result in download_results {
        match result {
            Ok(download_result) => {
                successful_downloads += 1;
                successful_media_ids.push(download_result.memory_item_id);
                sqlx::query(
                    "
                    UPDATE MemoryItem
                    SET status = ?1,
                        retry_count = 0,
                        last_error_code = NULL,
                        last_error_message = NULL
                    WHERE id = ?2
                    ",
                )
                .bind("downloaded")
                .bind(download_result.memory_item_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    format!(
                        "failed to update downloaded status for memory {}: {error}",
                        download_result.memory_item_id
                    )
                })?;

                let downloaded_files = i64::try_from(successful_downloads)
                    .map_err(|error| format!("failed to convert downloaded counter: {error}"))?;

                sqlx::query(
                    "
                    UPDATE ExportJob
                    SET downloaded_files = ?1
                    WHERE id = 1
                    ",
                )
                .bind(downloaded_files)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to update export job progress: {error}"))?;
            }
            Err(error) => {
                if let Some(memory_item_id) = error.memory_item_id() {
                    let next_retry_count = retry_counts_by_id
                        .get(&memory_item_id)
                        .copied()
                        .unwrap_or(0)
                        + 1;

                    let next_status = resolve_failed_memory_status(
                        &error.error_code(),
                        error.is_retryable(),
                        next_retry_count,
                    );

                    sqlx::query(
                        "
                        UPDATE MemoryItem
                        SET status = ?1,
                            retry_count = ?2,
                            last_error_code = ?3,
                            last_error_message = ?4
                        WHERE id = ?5
                        ",
                    )
                    .bind(next_status)
                    .bind(next_retry_count)
                    .bind(format!("{:?}", error.error_code()))
                    .bind(error.to_string())
                    .bind(memory_item_id)
                    .execute(&pool)
                    .await
                    .map_err(|db_error| {
                        format!(
                            "failed to update failed status for memory {}: {db_error}",
                            memory_item_id
                        )
                    })?;
                }
            }
        }
    }

    let overlay_tasks = successful_media_ids
        .iter()
        .filter_map(|memory_item_id| {
            let overlay_url = overlay_urls_by_id
                .get(memory_item_id)
                .and_then(|value| value.as_ref())?;
            let extension = extract_extension_from_url(overlay_url, "png");

            Some(core::downloader::DownloadTask {
                memory_item_id: *memory_item_id,
                url: overlay_url.clone(),
                destination_path: std::path::Path::new(&output_dir)
                    .join(format!("{memory_item_id}.overlay.{extension}")),
            })
        })
        .collect::<Vec<_>>();

    if !overlay_tasks.is_empty() {
        let overlay_results = core::downloader::download_tasks(overlay_tasks)
            .await
            .map_err(|error| format!("overlay download manager failed: {error}"))?;

        for result in overlay_results {
            if let Err(error) = result {
                if let Some(memory_item_id) = error.memory_item_id() {
                    sqlx::query(
                        "
                        UPDATE MemoryItem
                        SET last_error_code = ?1,
                            last_error_message = ?2
                        WHERE id = ?3 AND status = 'downloaded'
                        ",
                    )
                    .bind("OVERLAY_DOWNLOAD_FAILED")
                    .bind(error.to_string())
                    .bind(memory_item_id)
                    .execute(&pool)
                    .await
                    .map_err(|db_error| {
                        format!(
                            "failed to persist overlay warning for memory {}: {db_error}",
                            memory_item_id
                        )
                    })?;
                }
            }
        }
    }

    let remaining_retryable =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM MemoryItem WHERE status = 'retryable'")
            .fetch_one(&pool)
            .await
            .map_err(|error| format!("failed to query retryable memory count: {error}"))?;

    let remaining_expired =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM MemoryItem WHERE status = 'expired'")
            .fetch_one(&pool)
            .await
            .map_err(|error| format!("failed to query expired memory count: {error}"))?;

    let remaining_failed =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM MemoryItem WHERE status = 'failed'")
            .fetch_one(&pool)
            .await
            .map_err(|error| format!("failed to query failed memory count: {error}"))?;

    let final_status = if remaining_expired > 0 {
        "paused_expired"
    } else if remaining_retryable > 0 {
        "paused_retryable"
    } else if remaining_failed > 0 {
        "completed_with_failures"
    } else {
        "completed"
    };

    let downloaded_files = i64::try_from(successful_downloads)
        .map_err(|error| format!("failed to convert downloaded counter: {error}"))?;

    sqlx::query(
        "
        UPDATE ExportJob
        SET status = ?1,
            downloaded_files = ?2
        WHERE id = 1
        ",
    )
    .bind(final_status)
    .bind(downloaded_files)
    .execute(&pool)
    .await
    .map_err(|error| format!("failed to finalize export job state: {error}"))?;

    pool.close().await;
    Ok(successful_downloads)
}

#[tauri::command]
async fn resume_export_downloads(
    app: tauri::AppHandle,
    window: tauri::Window,
    output_dir: String,
    requests_per_minute: Option<u32>,
    concurrent_downloads: Option<u32>,
) -> Result<usize, String> {
    download_queued_memories(
        app,
        window,
        output_dir,
        requests_per_minute,
        concurrent_downloads,
    )
    .await
}

#[tauri::command]
async fn import_memories_json(
    app: tauri::AppHandle,
    json_content: String,
) -> Result<ImportMemoriesResult, String> {
    let database_url = memories_db_url(&app)?;

    let summary = core::parser::import_memories_history_json(&database_url, &json_content)
        .await
        .map_err(|error| format!("failed to import memories json: {error}"))?;

    Ok(ImportMemoriesResult {
        parsed_count: summary.parsed_count,
        imported_count: summary.imported_count,
        skipped_duplicates: summary.skipped_duplicates,
    })
}

#[tauri::command]
async fn validate_memory_file(path: String) -> Result<bool, String> {
    core::parser::validate_memories_history_file(std::path::Path::new(&path))
        .await
        .map(|_| true)
        .map_err(|error| format!("failed to validate memory file: {error}"))
}

#[tauri::command]
async fn apply_metadata_to_output_files(
    app: tauri::AppHandle,
    output_dir: String,
) -> Result<usize, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let rows = sqlx::query(
        "SELECT id, date, location FROM MemoryItem WHERE status IN ('downloaded', 'processed') ORDER BY id ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load metadata source rows from MemoryItem: {error}"))?;

    let output_path = std::path::Path::new(&output_dir);
    let mut applied_count = 0usize;

    for row in rows {
        let memory_item_id = row.get::<i64, _>("id");
        let date_taken = row.get::<String, _>("date");
        let location = row.get::<Option<String>, _>("location");

        let Some(file_path) = find_output_file_for_memory_item(output_path, memory_item_id)
            .map_err(|error| {
                format!(
                    "failed while locating output file for memory {}: {error}",
                    memory_item_id
                )
            })?
        else {
            continue;
        };

        core::media::write_metadata_with_ffmpeg(&file_path, &date_taken, location.as_deref())
            .await
            .map_err(|error| {
                format!(
                    "failed to write metadata for memory {} file '{}': {error}",
                    memory_item_id,
                    file_path.display()
                )
            })?;

        applied_count += 1;
    }

    pool.close().await;
    Ok(applied_count)
}

#[tauri::command]
async fn get_thumbnails(
    app: tauri::AppHandle,
    offset: i64,
    limit: i64,
) -> Result<Vec<ThumbnailItem>, String> {
    if offset < 0 {
        return Err("offset must be greater than or equal to 0".to_string());
    }

    if limit <= 0 {
        return Err("limit must be greater than 0".to_string());
    }

    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let rows = sqlx::query(
        "
        SELECT id
        FROM MemoryItem
        WHERE status = 'processed'
        ORDER BY id DESC
        LIMIT ?1 OFFSET ?2
        ",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to query processed memories for thumbnails: {error}"))?;

    pool.close().await;

    let thumbnails_dir = std::path::Path::new(".raw_cache").join(".thumbnails");

    let items = rows
        .into_iter()
        .filter_map(|row| {
            let memory_item_id = row.get::<i64, _>("id");
            let thumbnail_path = thumbnails_dir.join(format!("{memory_item_id}.webp"));

            if !thumbnail_path.exists() {
                return None;
            }

            Some(ThumbnailItem {
                memory_item_id,
                thumbnail_path: thumbnail_path.to_string_lossy().to_string(),
            })
        })
        .collect::<Vec<_>>();

    Ok(items)
}

#[tauri::command]
async fn process_downloaded_memories(
    app: tauri::AppHandle,
    window: tauri::Window,
    output_dir: String,
    keep_originals: bool,
) -> Result<ProcessMemoriesResult, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let rows = sqlx::query(
        "
        SELECT id, date, location
        FROM MemoryItem
        WHERE status IN ('downloaded', 'processing_failed')
        ORDER BY id ASC
        ",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load downloaded memories for processing: {error}"))?;

    let output_path = std::path::Path::new(&output_dir);
    let raw_cache_path = std::path::Path::new(".raw_cache");
    let thumbnail_path = output_path.join(".thumbnails");

    let mut processed_count = 0usize;
    let mut failed_count = 0usize;
    let total_files = rows.len();

    for (index, row) in rows.into_iter().enumerate() {
        let memory_item_id = row.get::<i64, _>("id");
        let date_taken = row.get::<String, _>("date");
        let location = row.get::<Option<String>, _>("location");

        let raw_media_path = find_output_file_for_memory_item(raw_cache_path, memory_item_id)
            .map_err(|error| {
                format!(
                    "failed while locating downloaded file for memory {}: {error}",
                    memory_item_id
                )
            })?;

        let Some(raw_media_path) = raw_media_path else {
            failed_count += 1;
            sqlx::query(
                "
                UPDATE MemoryItem
                SET status = 'processing_failed',
                    last_error_code = 'MISSING_DOWNLOADED_FILE',
                    last_error_message = 'downloaded source file is missing'
                WHERE id = ?1
                ",
            )
            .bind(memory_item_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to update missing-file status for memory {}: {error}",
                    memory_item_id
                )
            })?;

            window
                .emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(memory_item_id),
                        status: "error".to_string(),
                        error_message: Some("downloaded source file is missing".to_string()),
                    },
                )
                .map_err(|error| format!("failed to emit processing progress event: {error}"))?;
            continue;
        };

        let overlay_path =
            find_overlay_file_for_memory_item(raw_cache_path, memory_item_id).map_err(|error| {
                format!(
                    "failed while locating overlay file for memory {}: {error}",
                    memory_item_id
                )
            })?;

        if let Err(error) = core::processor::process_media(core::processor::ProcessMediaInput {
            memory_item_id,
            raw_media_paths: vec![raw_media_path.clone()],
            overlay_path,
            date_taken,
            location,
            export_dir: output_path.to_path_buf(),
            thumbnail_dir: thumbnail_path.clone(),
            keep_originals,
        })
        .await
        {
            failed_count += 1;
            sqlx::query(
                "
                UPDATE MemoryItem
                SET status = 'processing_failed',
                    last_error_code = 'PROCESSING_FAILED',
                    last_error_message = ?1
                WHERE id = ?2
                ",
            )
            .bind(error.to_string())
            .bind(memory_item_id)
            .execute(&pool)
            .await
            .map_err(|db_error| {
                format!(
                    "failed to update metadata failure status for memory {}: {db_error}",
                    memory_item_id
                )
            })?;

            window
                .emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(memory_item_id),
                        status: "error".to_string(),
                        error_message: Some(error.to_string()),
                    },
                )
                .map_err(|emit_error| {
                    format!("failed to emit processing progress event: {emit_error}")
                })?;
            continue;
        }

        processed_count += 1;
        sqlx::query(
            "
            UPDATE MemoryItem
            SET status = 'processed',
                last_error_code = NULL,
                last_error_message = NULL
            WHERE id = ?1
            ",
        )
        .bind(memory_item_id)
        .execute(&pool)
        .await
        .map_err(|error| {
            format!(
                "failed to update processed status for memory {}: {error}",
                memory_item_id
            )
        })?;

        window
            .emit(
                PROCESS_PROGRESS_EVENT,
                ProcessProgressPayload {
                    total_files,
                    completed_files: index + 1,
                    successful_files: processed_count,
                    failed_files: failed_count,
                    memory_item_id: Some(memory_item_id),
                    status: "success".to_string(),
                    error_message: None,
                },
            )
            .map_err(|error| format!("failed to emit processing progress event: {error}"))?;
    }

    pool.close().await;
    Ok(ProcessMemoriesResult {
        processed_count,
        failed_count,
    })
}

fn find_output_file_for_memory_item(
    output_dir: &std::path::Path,
    memory_item_id: i64,
) -> Result<Option<std::path::PathBuf>, std::io::Error> {
    if !output_dir.exists() {
        return Ok(None);
    }

    let target_stem = memory_item_id.to_string();

    for entry in std::fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };

        if stem == target_stem {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn find_overlay_file_for_memory_item(
    output_dir: &std::path::Path,
    memory_item_id: i64,
) -> Result<Option<std::path::PathBuf>, std::io::Error> {
    if !output_dir.exists() {
        return Ok(None);
    }

    let prefix = format!("{memory_item_id}.overlay");

    for entry in std::fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };

        if stem == prefix {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn extract_extension_from_url(url: &str, fallback: &str) -> String {
    url.rsplit_once('.')
        .map(|(_, ext)| ext)
        .and_then(|ext| ext.split(['?', '#']).next())
        .filter(|ext| !ext.is_empty() && ext.len() <= 8)
        .unwrap_or(fallback)
        .to_string()
}

fn resolve_failed_memory_status(
    error_code: &core::downloader::DownloadErrorCode,
    is_retryable: bool,
    next_retry_count: i64,
) -> &'static str {
    if matches!(error_code, core::downloader::DownloadErrorCode::ExpiredLink) {
        return "expired";
    }

    if is_retryable && next_retry_count < MAX_PERSISTED_RETRY_ATTEMPTS {
        return "retryable";
    }

    "failed"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:memories.db", db::sqlite_migrations())
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            greet,
            get_job_state,
            set_job_state,
            import_memories_json,
            validate_memory_file,
            download_queued_memories,
            resume_export_downloads,
            apply_metadata_to_output_files,
            process_downloaded_memories,
            get_thumbnails
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::resolve_failed_memory_status;
    use crate::core::downloader::DownloadErrorCode;

    #[test]
    fn expired_errors_always_pause_for_json_refresh() {
        assert_eq!(
            resolve_failed_memory_status(&DownloadErrorCode::ExpiredLink, true, 1),
            "expired"
        );
        assert_eq!(
            resolve_failed_memory_status(&DownloadErrorCode::ExpiredLink, false, 10),
            "expired"
        );
    }

    #[test]
    fn retryable_errors_move_to_retryable_before_threshold() {
        assert_eq!(
            resolve_failed_memory_status(&DownloadErrorCode::HttpError, true, 1),
            "retryable"
        );
        assert_eq!(
            resolve_failed_memory_status(&DownloadErrorCode::ConcurrencyError, true, 2),
            "retryable"
        );
    }

    #[test]
    fn non_retryable_or_threshold_reached_move_to_failed() {
        assert_eq!(
            resolve_failed_memory_status(&DownloadErrorCode::IoError, false, 1),
            "failed"
        );
        assert_eq!(
            resolve_failed_memory_status(&DownloadErrorCode::HttpError, true, 3),
            "failed"
        );
    }
}
