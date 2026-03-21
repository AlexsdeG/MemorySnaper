pub mod core;
pub mod db;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Row;
use tauri::{Emitter, Manager};

const MAX_PERSISTED_RETRY_ATTEMPTS: i64 = 3;
const PROCESS_PROGRESS_EVENT: &str = "process-progress";
const SESSION_LOG_EVENT: &str = "session-log";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportJobState {
    status: String,
    total_files: i64,
    downloaded_files: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionLogPayload {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ZipSessionInitResult {
    job_id: String,
    active_zip: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessingSessionOverview {
    job_id: Option<String>,
    export_status: String,
    total_files: i64,
    downloaded_files: i64,
    processed_files: i64,
    duplicates_skipped: i64,
    is_paused: bool,
    is_stopped: bool,
    active_zip: Option<String>,
    finished_zip_files: Vec<String>,
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

#[derive(Debug, Clone)]
struct ProcessUnit {
    memory_group_id: Option<i64>,
    progress_item_id: i64,
    memory_item_ids: Vec<i64>,
    date_taken: String,
    location: Option<String>,
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
    error_code: Option<ProcessErrorCode>,
    error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ProcessErrorCode {
    MissingDownloadedFile,
    ProcessingFailed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailItem {
    memory_item_id: i64,
    thumbnail_path: String,
}

fn emit_session_log(window: &tauri::Window, message: impl Into<String>) -> Result<(), String> {
    window
        .emit(
            SESSION_LOG_EVENT,
            SessionLogPayload {
                message: message.into(),
            },
        )
        .map_err(|error| format!("failed to emit session log event: {error}"))
}

fn parse_snapchat_zip_name(file_stem: &str) -> Result<(String, Option<u32>), String> {
    let prefix = "mydata~";
    if !file_stem.starts_with(prefix) {
        return Err(format!(
            "zip '{file_stem}' must start with '{prefix}'"
        ));
    }

    let rest = &file_stem[prefix.len()..];
    if rest.is_empty() {
        return Err(format!("zip '{file_stem}' is missing uuid segment"));
    }

    let (uuid_part, part_number) = if let Some((uuid_candidate, number_candidate)) = rest.rsplit_once(' ') {
        if number_candidate.chars().all(|character| character.is_ascii_digit()) {
            let parsed_number = number_candidate
                .parse::<u32>()
                .map_err(|error| format!("invalid zip part number in '{file_stem}': {error}"))?;
            (uuid_candidate, Some(parsed_number))
        } else {
            (rest, None)
        }
    } else {
        (rest, None)
    };

    let is_uuid_like = !uuid_part.is_empty()
        && uuid_part
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-');

    if !is_uuid_like {
        return Err(format!(
            "zip '{file_stem}' has invalid uuid segment '{uuid_part}'"
        ));
    }

    Ok((uuid_part.to_string(), part_number))
}

fn zip_contains_required_memories_entry(zip_path: &std::path::Path) -> Result<bool, String> {
    let file = std::fs::File::open(zip_path).map_err(|error| {
        format!(
            "failed to open zip '{}' for base verification: {error}",
            zip_path.display()
        )
    })?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("failed to read zip archive '{}': {error}", zip_path.display()))?;

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("failed to inspect zip entry {}: {error}", index))?;

        let normalized_name = entry.name().replace('\\', "/").to_ascii_lowercase();
        if normalized_name == "json/memories_history.json"
            || normalized_name.ends_with("/json/memories_history.json")
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn zip_contains_memories_folder(zip_path: &std::path::Path) -> Result<bool, String> {
    let file = std::fs::File::open(zip_path).map_err(|error| {
        format!(
            "failed to open zip '{}' for memories folder verification: {error}",
            zip_path.display()
        )
    })?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("failed to read zip archive '{}': {error}", zip_path.display()))?;

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("failed to inspect zip entry {}: {error}", index))?;

        let normalized_name = entry.name().replace('\\', "/").to_ascii_lowercase();
        if normalized_name == "memories"
            || normalized_name.starts_with("memories/")
            || normalized_name.contains("/memories/")
            || normalized_name.ends_with("/memories")
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn ensure_first_zip_is_base_archive(zip_path: &std::path::Path) -> Result<(), String> {
    let stem = zip_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let (_, part_number) = parse_snapchat_zip_name(stem)?;

    if part_number.is_some() {
        return Err(format!(
            "first zip '{}' must be the base archive without trailing part numbers",
            zip_path.display()
        ));
    }

    Ok(())
}

async fn latest_export_job_id(pool: &sqlx::SqlitePool) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM ExportJobs ORDER BY datetime(created_at) DESC, id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to read latest export job id: {error}"))
}

fn memories_db_url(app: &tauri::AppHandle) -> Result<String, String> {
    let mut app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;

    std::fs::create_dir_all(&app_data_dir)
        .map_err(|error| format!("failed to create app data directory: {error}"))?;

    app_data_dir.push("memories.db");

    Ok(format!("sqlite://{}", app_data_dir.to_string_lossy()))
}

fn resolve_output_dir(app: &tauri::AppHandle, output_dir: &str) -> Result<std::path::PathBuf, String> {
    let requested_path = std::path::PathBuf::from(output_dir);
    if requested_path.is_absolute() {
        return Ok(requested_path);
    }

    let mut app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory for output dir: {error}"))?;

    std::fs::create_dir_all(&app_data_dir)
        .map_err(|error| format!("failed to create app data directory for output dir: {error}"))?;

    app_data_dir.push(requested_path);
    Ok(app_data_dir)
}

async fn sqlite_column_exists(
    pool: &sqlx::SqlitePool,
    table_name: &str,
    column_name: &str,
) -> Result<bool, String> {
    let pragma_sql = format!("PRAGMA table_info({table_name})");
    let rows = sqlx::query(&pragma_sql)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to inspect table {table_name}: {error}"))?;

    Ok(rows
        .iter()
        .any(|row| row.get::<String, _>("name") == column_name))
}

async fn ensure_sqlite_column(
    pool: &sqlx::SqlitePool,
    table_name: &str,
    column_name: &str,
    column_definition: &str,
) -> Result<(), String> {
    if sqlite_column_exists(pool, table_name, column_name).await? {
        return Ok(());
    }

    let alter_sql = format!("ALTER TABLE {table_name} ADD COLUMN {column_definition}");
    sqlx::query(&alter_sql)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to add column {column_name} to table {table_name}: {error}"
            )
        })?;

    Ok(())
}

    async fn setup_database(app: &tauri::AppHandle) -> Result<(), String> {
        let mut app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("failed to resolve app data directory: {error}"))?;

        std::fs::create_dir_all(&app_data_dir)
            .map_err(|error| format!("failed to create app data directory: {error}"))?;

        app_data_dir.push("memories.db");

        let connect_options = SqliteConnectOptions::new()
            .filename(&app_data_dir)
            .create_if_missing(true);

        let pool = sqlx::SqlitePool::connect_with(connect_options)
            .await
            .map_err(|error| format!("failed to connect to memories database for setup: {error}"))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS MemoryItem (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                location TEXT,
                media_url TEXT NOT NULL,
                overlay_url TEXT,
                status TEXT NOT NULL,
                retry_count INTEGER NOT NULL DEFAULT 0,
                last_error_code TEXT,
                last_error_message TEXT
            )",
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to create MemoryItem table: {error}"))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ExportJob (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                status TEXT NOT NULL,
                total_files INTEGER NOT NULL DEFAULT 0,
                downloaded_files INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to create ExportJob table: {error}"))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS Memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hash TEXT NOT NULL UNIQUE,
                date TEXT NOT NULL,
                status TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to create Memories table: {error}"))?;

        ensure_sqlite_column(&pool, "Memories", "job_id", "job_id TEXT").await?;
        ensure_sqlite_column(&pool, "Memories", "mid", "mid TEXT").await?;
        ensure_sqlite_column(&pool, "Memories", "content_hash", "content_hash TEXT").await?;
        ensure_sqlite_column(&pool, "Memories", "relative_path", "relative_path TEXT").await?;
        ensure_sqlite_column(&pool, "Memories", "thumbnail_path", "thumbnail_path TEXT").await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_content_hash ON Memories(content_hash)",
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to create Memories content hash index: {error}"))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS MediaChunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id INTEGER NOT NULL,
                url TEXT NOT NULL,
                overlay_url TEXT,
                order_index INTEGER NOT NULL,
                FOREIGN KEY (memory_id) REFERENCES Memories(id)
            )",
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to create MediaChunks table: {error}"))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ExportJobs (
                id TEXT PRIMARY KEY,
                created_at DATETIME,
                status TEXT
            )",
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to create ExportJobs table: {error}"))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ProcessedZips (
                job_id TEXT,
                filename TEXT,
                status TEXT,
                PRIMARY KEY (job_id, filename)
            )",
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to create ProcessedZips table: {error}"))?;

        pool.close().await;
        Ok(())
    }

    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_system_locale() -> Option<String> {
    sys_locale::get_locale()
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
async fn get_queued_count(app: tauri::AppHandle) -> Result<i64, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM MemoryItem WHERE status IN ('queued', 'retryable')",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("failed to count queued memory items: {error}"))?;

    pool.close().await;
    Ok(count)
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
    let resolved_output_dir = resolve_output_dir(&app, &output_dir)?;

    eprintln!(
        "[downloader-debug] download_queued_memories start output_dir='{}' resolved_output_dir='{}' requests_per_minute={:?} concurrent_downloads={:?}",
        output_dir,
        resolved_output_dir.display(),
        requests_per_minute,
        concurrent_downloads
    );

    emit_session_log(
        &window,
        format!(
            "[{}] Download session started (output: {})",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            resolved_output_dir.display()
        ),
    )?;

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

    eprintln!(
        "[downloader-debug] queued rows loaded total_files={} resolved_output_dir='{}'",
        total_files,
        resolved_output_dir.display()
    );

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
                destination_path: resolved_output_dir
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

    eprintln!(
        "[downloader-debug] download manager returned result_count={}",
        download_results.len()
    );

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
                eprintln!(
                    "[downloader-debug] item failed memory_item_id={:?} error_code={:?} retryable={} http_status={:?} url={:?} error={}",
                    error.memory_item_id(),
                    error.error_code(),
                    error.is_retryable(),
                    error.http_status(),
                    error.url(),
                    error
                );

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

                    eprintln!(
                        "[downloader-debug] item status transition memory_item_id={} next_retry_count={} next_status={}",
                        memory_item_id, next_retry_count, next_status
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
                destination_path: resolved_output_dir
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
                eprintln!(
                    "[downloader-debug] overlay download failed memory_item_id={:?} error_code={:?} http_status={:?} url={:?} error={}",
                    error.memory_item_id(),
                    error.error_code(),
                    error.http_status(),
                    error.url(),
                    error
                );

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

    eprintln!(
        "[downloader-debug] download_queued_memories complete successful_downloads={} remaining_retryable={} remaining_expired={} remaining_failed={} final_status={}",
        successful_downloads,
        remaining_retryable,
        remaining_expired,
        remaining_failed,
        final_status
    );

    emit_session_log(
        &window,
        format!(
            "[{}] Download phase finished: {} success, {} retryable, {} expired, {} failed",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            successful_downloads,
            remaining_retryable,
            remaining_expired,
            remaining_failed
        ),
    )?;

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
async fn validate_memory_json_content(json_content: String) -> Result<bool, String> {
    core::parser::validate_memories_history_json_content(&json_content)
        .map(|_| true)
        .map_err(|error| format!("failed to validate memories json content: {error}"))
}

#[tauri::command]
async fn validate_base_zip_archive(path: String) -> Result<bool, String> {
    let zip_path = std::path::PathBuf::from(path);
    ensure_first_zip_is_base_archive(&zip_path)?;

    let has_json = zip_contains_required_memories_entry(&zip_path)?;
    let has_memories_folder = zip_contains_memories_folder(&zip_path)?;

    Ok(has_json && has_memories_folder)
}

#[tauri::command]
async fn import_memories_from_zip(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<ImportMemoriesResult, String> {
    let database_url = memories_db_url(&app)?;
    let zip_path = std::path::PathBuf::from(zip_path);

    let json_content = core::parser::load_memories_history_json(&zip_path)
        .await
        .map_err(|error| format!("failed to read memories_history.json from zip: {error}"))?;

    let summary = core::parser::import_memories_history_json(&database_url, &json_content)
        .await
        .map_err(|error| format!("failed to import memories from zip: {error}"))?;

    Ok(ImportMemoriesResult {
        parsed_count: summary.parsed_count,
        imported_count: summary.imported_count,
        skipped_duplicates: summary.skipped_duplicates,
    })
}

#[tauri::command]
async fn initialize_zip_session(
    app: tauri::AppHandle,
    zip_paths: Vec<String>,
) -> Result<ZipSessionInitResult, String> {
    if zip_paths.is_empty() {
        return Err("at least one zip path is required".to_string());
    }

    let parsed_zip_paths: Vec<std::path::PathBuf> = zip_paths
        .iter()
        .map(std::path::PathBuf::from)
        .collect();

    let mut main_zip: Option<(std::path::PathBuf, String)> = None;
    let mut optional_part_zips = Vec::<(std::path::PathBuf, String, u32)>::new();

    for zip_path in &parsed_zip_paths {
        let stem = zip_path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("zip '{}' has an invalid file name", zip_path.display()))?;

        let (uuid, part_number) = parse_snapchat_zip_name(stem)?;

        if !zip_contains_memories_folder(zip_path)? {
            return Err(format!(
                "zip '{}' must contain a memories/ folder",
                zip_path.display()
            ));
        }

        match part_number {
            None => {
                if main_zip.is_some() {
                    return Err("multiple main zip files detected; provide exactly one mydata~<uuid>.zip".to_string());
                }
                main_zip = Some((zip_path.clone(), uuid));
            }
            Some(part_number) => {
                optional_part_zips.push((zip_path.clone(), uuid, part_number));
            }
        }
    }

    let (main_zip_path, main_uuid) = main_zip
        .ok_or_else(|| "missing main zip file; provide mydata~<uuid>.zip as the base archive".to_string())?;

    if !zip_contains_required_memories_entry(&main_zip_path)? {
        return Err(format!(
            "first zip '{}' must contain json/memories_history.json",
            main_zip_path.display()
        ));
    }

    for (_, uuid, _) in &optional_part_zips {
        if uuid != &main_uuid {
            return Err("all optional zip parts must belong to the same mydata~<uuid> set as the main zip".to_string());
        }
    }

    optional_part_zips.sort_by_key(|(_, _, part_number)| *part_number);

    let mut ordered_zip_paths = Vec::with_capacity(parsed_zip_paths.len());
    ordered_zip_paths.push(main_zip_path.clone());
    for (zip_path, _, _) in &optional_part_zips {
        ordered_zip_paths.push(zip_path.clone());
    }

    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let job_id = format!("job-{}", chrono::Utc::now().timestamp_millis());

    sqlx::query(
        "
        INSERT INTO ExportJobs (id, created_at, status)
        VALUES (?1, datetime('now'), 'RUNNING')
        ",
    )
    .bind(&job_id)
    .execute(&pool)
    .await
    .map_err(|error| format!("failed to initialize ExportJobs row: {error}"))?;

    for (index, zip_path) in ordered_zip_paths.iter().enumerate() {
        let file_name = zip_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();

        let status = if index == 0 { "processing" } else { "pending" };

        sqlx::query(
            "
            INSERT INTO ProcessedZips (job_id, filename, status)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(job_id, filename) DO UPDATE SET status = excluded.status
            ",
        )
        .bind(&job_id)
        .bind(file_name)
        .bind(status)
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to initialize ProcessedZips row: {error}"))?;
    }

    pool.close().await;

    Ok(ZipSessionInitResult {
        job_id,
        active_zip: ordered_zip_paths
            .first()
            .and_then(|path| path.file_name())
            .and_then(|value| value.to_str())
            .map(str::to_string),
    })
}

#[tauri::command]
async fn finalize_zip_session(app: tauri::AppHandle, job_id: Option<String>) -> Result<(), String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let target_job_id = if let Some(job_id) = job_id {
        Some(job_id)
    } else {
        latest_export_job_id(&pool).await?
    };

    if let Some(target_job_id) = target_job_id {
        sqlx::query(
            "
            UPDATE ProcessedZips
            SET status = 'finished'
            WHERE job_id = ?1
            ",
        )
        .bind(&target_job_id)
        .execute(&pool)
        .await
        .map_err(|error| format!("failed to finalize ProcessedZips status: {error}"))?;

        sqlx::query("UPDATE ExportJobs SET status = 'COMPLETED' WHERE id = ?1")
            .bind(target_job_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to finalize ExportJobs status: {error}"))?;
    }

    pool.close().await;
    Ok(())
}

#[tauri::command]
fn set_processing_paused(paused: bool) -> Result<(), String> {
    core::state::set_paused(paused);
    if paused {
        core::state::set_stopped(false);
    }
    Ok(())
}

#[tauri::command]
fn stop_processing_session() -> Result<(), String> {
    core::state::set_stopped(true);
    core::state::set_paused(false);
    Ok(())
}

#[tauri::command]
fn resume_processing_session() -> Result<(), String> {
    core::state::set_stopped(false);
    core::state::set_paused(false);
    Ok(())
}

#[tauri::command]
async fn get_processing_session_overview(
    app: tauri::AppHandle,
) -> Result<ProcessingSessionOverview, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let job_row = sqlx::query(
        "SELECT status, total_files, downloaded_files FROM ExportJob ORDER BY id ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("failed to read ExportJob status: {error}"))?;

    let latest_job_id = latest_export_job_id(&pool).await?;

    let processed_files = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM MemoryItem WHERE status = 'processed'",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("failed to read processed files count: {error}"))?;

    let duplicates_skipped = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM MemoryItem WHERE status = 'duplicate'",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("failed to read duplicate files count: {error}"))?;

    let imported_files = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM MemoryItem")
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("failed to read imported files count: {error}"))?;

    let (active_zip, finished_zip_files) = if let Some(job_id) = latest_job_id.as_deref() {
        let active_zip = sqlx::query_scalar::<_, String>(
            "SELECT filename FROM ProcessedZips WHERE job_id = ?1 AND status = 'processing' ORDER BY filename ASC LIMIT 1",
        )
        .bind(job_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("failed to read active zip status: {error}"))?;

        let finished_zip_files = sqlx::query_scalar::<_, String>(
            "SELECT filename FROM ProcessedZips WHERE job_id = ?1 AND status = 'finished' ORDER BY filename ASC",
        )
        .bind(job_id)
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("failed to read finished zip status: {error}"))?;

        (active_zip, finished_zip_files)
    } else {
        (None, Vec::new())
    };

    let state = core::state::snapshot();
    pool.close().await;

    let (export_status, total_files, downloaded_files) = if let Some(row) = job_row {
        let row_total_files = row.get::<i64, _>("total_files");
        let row_downloaded_files = row.get::<i64, _>("downloaded_files");

        (
            row.get::<String, _>("status"),
            if row_total_files > 0 {
                row_total_files
            } else {
                imported_files
            },
            if row_downloaded_files > 0 {
                row_downloaded_files
            } else {
                processed_files
            },
        )
    } else {
        ("idle".to_string(), imported_files, processed_files)
    };

    Ok(ProcessingSessionOverview {
        job_id: latest_job_id,
        export_status,
        total_files,
        downloaded_files,
        processed_files,
        duplicates_skipped,
        is_paused: state.is_paused,
        is_stopped: state.is_stopped,
        active_zip,
        finished_zip_files,
    })
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

    let thumbnails_dir = resolve_output_dir(&app, ".raw_cache")?.join(".thumbnails");

    let items = rows
        .into_iter()
        .filter_map(|row| {
            let memory_item_id = row.get::<i64, _>("id");
            let thumbnail_path = thumbnails_dir.join(format!("{memory_item_id}.webp"));

            if !thumbnail_path.exists() {
                return None;
            }

            let resolved_thumbnail_path = std::fs::canonicalize(&thumbnail_path)
                .unwrap_or(thumbnail_path);

            Some(ThumbnailItem {
                memory_item_id,
                thumbnail_path: resolved_thumbnail_path.to_string_lossy().to_string(),
            })
        })
        .collect::<Vec<_>>();

    Ok(items)
}

#[tauri::command]
async fn reset_all_app_data(app: tauri::AppHandle) -> Result<(), String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| format!("failed to begin reset transaction: {error}"))?;

    sqlx::query("DELETE FROM MediaChunks")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("failed to clear MediaChunks table: {error}"))?;

    sqlx::query("DELETE FROM ProcessedZips")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("failed to clear ProcessedZips table: {error}"))?;

    sqlx::query("DELETE FROM Memories")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("failed to clear Memories table: {error}"))?;

    sqlx::query("DELETE FROM MemoryItem")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("failed to clear MemoryItem table: {error}"))?;

    sqlx::query("DELETE FROM ExportJob")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("failed to clear ExportJob table: {error}"))?;

    sqlx::query("DELETE FROM ExportJobs")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("failed to clear ExportJobs table: {error}"))?;

    sqlx::query(
        "DELETE FROM sqlite_sequence WHERE name IN ('MediaChunks', 'ProcessedZips', 'Memories', 'MemoryItem', 'ExportJob', 'ExportJobs')",
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("failed to reset autoincrement counters: {error}"))?;

    transaction
        .commit()
        .await
        .map_err(|error| format!("failed to commit reset transaction: {error}"))?;

    pool.close().await;

    let raw_cache_path = resolve_output_dir(&app, ".raw_cache")?;
    let thumbnails_cache_path = raw_cache_path.join(".thumbnails");
    let cache_paths = [raw_cache_path.as_path(), thumbnails_cache_path.as_path()];

    for cache_path in cache_paths {
        if !cache_path.exists() {
            continue;
        }

        std::fs::remove_dir_all(cache_path).map_err(|error| {
            format!(
                "failed to clear cache directory '{}': {error}",
                cache_path.display()
            )
        })?;
    }

    std::fs::create_dir_all(&raw_cache_path)
        .map_err(|error| format!("failed to recreate .raw_cache directory: {error}"))?;

    Ok(())
}

#[tauri::command]
async fn process_downloaded_memories(
    app: tauri::AppHandle,
    window: tauri::Window,
    output_dir: String,
    keep_originals: bool,
) -> Result<ProcessMemoriesResult, String> {
    let resolved_output_dir = resolve_output_dir(&app, &output_dir)?;

    eprintln!(
        "[processor-debug] process_downloaded_memories start output_dir='{}' resolved_output_dir='{}' keep_originals={}",
        output_dir,
        resolved_output_dir.display(),
        keep_originals
    );

    emit_session_log(
        &window,
        format!(
            "[{}] Processing phase started",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ),
    )?;

    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let chunk_rows = sqlx::query(
        "
        SELECT
            mc.memory_id AS memory_id,
            mc.order_index AS order_index,
            mi.id AS memory_item_id,
            mi.date AS date,
            mi.location AS location
        FROM MediaChunks mc
        JOIN MemoryItem mi
            ON mi.media_url = mc.url
           AND IFNULL(mi.overlay_url, '') = IFNULL(mc.overlay_url, '')
        WHERE mi.status IN ('downloaded', 'processing_failed')
        ORDER BY mc.memory_id ASC, mc.order_index ASC, mi.id ASC
        ",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load grouped downloaded memories for processing: {error}"))?;

    let process_units: Vec<ProcessUnit> = if chunk_rows.is_empty() {
        eprintln!(
            "[processor-debug] no MediaChunks groups found, falling back to per-item processing"
        );

        let fallback_rows = sqlx::query(
            "
            SELECT id, date, location
            FROM MemoryItem
            WHERE status IN ('downloaded', 'processing_failed')
            ORDER BY id ASC
            ",
        )
        .fetch_all(&pool)
        .await
        .map_err(|error| {
            format!("failed to load downloaded memories for fallback processing: {error}")
        })?;

        fallback_rows
            .into_iter()
            .map(|row| {
                let memory_item_id = row.get::<i64, _>("id");
                ProcessUnit {
                    memory_group_id: None,
                    progress_item_id: memory_item_id,
                    memory_item_ids: vec![memory_item_id],
                    date_taken: row.get::<String, _>("date"),
                    location: row.get::<Option<String>, _>("location"),
                }
            })
            .collect()
    } else {
        let mut grouped_units = std::collections::BTreeMap::<i64, ProcessUnit>::new();

        for row in chunk_rows {
            let memory_group_id = row.get::<i64, _>("memory_id");
            let memory_item_id = row.get::<i64, _>("memory_item_id");
            let date_taken = row.get::<String, _>("date");
            let location = row.get::<Option<String>, _>("location");

            let entry = grouped_units.entry(memory_group_id).or_insert_with(|| ProcessUnit {
                memory_group_id: Some(memory_group_id),
                progress_item_id: memory_item_id,
                memory_item_ids: Vec::new(),
                date_taken,
                location,
            });

            if !entry.memory_item_ids.contains(&memory_item_id) {
                entry.memory_item_ids.push(memory_item_id);
            }
        }

        grouped_units.into_values().collect()
    };

    let output_path = resolved_output_dir.as_path();
    let raw_cache_path = resolved_output_dir.as_path();
    let thumbnail_path = output_path.join(".thumbnails");

    let mut processed_count = 0usize;
    let mut failed_count = 0usize;
    let total_files = process_units.len();

    eprintln!(
        "[processor-debug] process units loaded total_units={} resolved_output_dir='{}'",
        total_files,
        resolved_output_dir.display()
    );

    for (index, unit) in process_units.iter().enumerate() {
        emit_session_log(
            &window,
            format!(
                "[{}] Processing date {} item {} ({}/{})",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                unit.date_taken,
                unit.progress_item_id,
                index + 1,
                total_files
            ),
        )?;

        if wait_for_pause_or_stop_and_mark_pending(&pool, &process_units[index..]).await? {
            eprintln!(
                "[processor-debug] stop requested; leaving remaining units pending (remaining={})",
                process_units.len().saturating_sub(index)
            );
            break;
        }

        let mut raw_media_paths = Vec::with_capacity(unit.memory_item_ids.len());
        let mut missing_file_for_item: Option<i64> = None;

        for memory_item_id in &unit.memory_item_ids {
            let raw_media_path = find_output_file_for_memory_item(raw_cache_path, *memory_item_id)
                .map_err(|error| {
                    format!(
                        "failed while locating downloaded file for memory {}: {error}",
                        memory_item_id
                    )
                })?;

            let Some(raw_media_path) = raw_media_path else {
                missing_file_for_item = Some(*memory_item_id);
                break;
            };

            raw_media_paths.push(raw_media_path);
        }

        if let Some(missing_memory_item_id) = missing_file_for_item {
            failed_count += 1;

            eprintln!(
                "[processor-debug] missing raw file memory_group={:?} progress_item_id={} missing_memory_item_id={}",
                unit.memory_group_id,
                unit.progress_item_id,
                missing_memory_item_id
            );

            for memory_item_id in &unit.memory_item_ids {
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
            }

            if let Some(memory_group_id) = unit.memory_group_id {
                sqlx::query("UPDATE Memories SET status = 'processing_failed' WHERE id = ?1")
                    .bind(memory_group_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| {
                        format!(
                            "failed to update processing_failed status for memory group {}: {error}",
                            memory_group_id
                        )
                    })?;
            }

            window
                .emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(unit.progress_item_id),
                        status: "error".to_string(),
                        error_code: Some(ProcessErrorCode::MissingDownloadedFile),
                        error_message: Some("downloaded source file is missing".to_string()),
                    },
                )
                .map_err(|error| format!("failed to emit processing progress event: {error}"))?;
            continue;
        }

        let overlay_path = {
            let mut found_overlay_path = None;

            for memory_item_id in &unit.memory_item_ids {
                let candidate_overlay_path =
                    find_overlay_file_for_memory_item(raw_cache_path, *memory_item_id).map_err(
                        |error| {
                            format!(
                                "failed while locating overlay file for memory {}: {error}",
                                memory_item_id
                            )
                        },
                    )?;

                if candidate_overlay_path.is_some() {
                    found_overlay_path = candidate_overlay_path;
                    break;
                }
            }

            found_overlay_path
        };

        eprintln!(
            "[processor-debug] processing unit memory_group={:?} progress_item_id={} chunk_count={} overlay_present={}",
            unit.memory_group_id,
            unit.progress_item_id,
            raw_media_paths.len(),
            overlay_path.is_some()
        );

        let process_result = core::processor::process_media(core::processor::ProcessMediaInput {
            memory_item_id: unit.progress_item_id,
            memory_group_id: unit.memory_group_id,
            raw_media_paths,
            overlay_path,
            date_taken: unit.date_taken.clone(),
            location: unit.location.clone(),
            export_dir: output_path.to_path_buf(),
            thumbnail_dir: thumbnail_path.clone(),
            keep_originals,
            database_url: database_url.clone(),
        })
        .await;

        if let Err(error) = process_result {
            failed_count += 1;

            eprintln!(
                "[processor-debug] processing failure memory_group={:?} progress_item_id={} error={}",
                unit.memory_group_id,
                unit.progress_item_id,
                error
            );

            for memory_item_id in &unit.memory_item_ids {
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
            }

            if let Some(memory_group_id) = unit.memory_group_id {
                sqlx::query("UPDATE Memories SET status = 'processing_failed' WHERE id = ?1")
                    .bind(memory_group_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| {
                        format!(
                            "failed to update processing_failed status for memory group {}: {error}",
                            memory_group_id
                        )
                    })?;
            }

            window
                .emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(unit.progress_item_id),
                        status: "error".to_string(),
                        error_code: Some(ProcessErrorCode::ProcessingFailed),
                        error_message: Some(error.to_string()),
                    },
                )
                .map_err(|emit_error| {
                    format!("failed to emit processing progress event: {emit_error}")
                })?;
            continue;
        }

        if let Ok(core::processor::ProcessMediaResult::Duplicate { ref content_hash }) = process_result {
            eprintln!(
                "[processor-debug] duplicate skipped memory_item_id={} hash={content_hash}",
                unit.progress_item_id
            );

            for memory_item_id in &unit.memory_item_ids {
                sqlx::query(
                    "
                    UPDATE MemoryItem
                    SET status = 'duplicate',
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
                        "failed to update duplicate status for memory {}: {error}",
                        memory_item_id
                    )
                })?;
            }

            window
                .emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(unit.progress_item_id),
                        status: "duplicate".to_string(),
                        error_code: None,
                        error_message: None,
                    },
                )
                .map_err(|emit_error| {
                    format!("failed to emit processing progress event: {emit_error}")
                })?;

            continue;
        }

        let processed_output = match process_result {
            Ok(core::processor::ProcessMediaResult::Processed(output)) => output,
            Ok(core::processor::ProcessMediaResult::Duplicate { .. }) => unreachable!(),
            Err(_) => unreachable!(),
        };

        let relative_media_path = path_to_relative_string(&processed_output.final_media_path, output_path)
            .map_err(|error| {
                format!(
                    "failed to compute relative media path for memory {}: {error}",
                    unit.progress_item_id
                )
            })?;
        let relative_thumbnail_path =
            path_to_relative_string(&processed_output.thumbnail_path, output_path).map_err(|error| {
                format!(
                    "failed to compute relative thumbnail path for memory {}: {error}",
                    unit.progress_item_id
                )
            })?;

        processed_count += 1;

        for memory_item_id in &unit.memory_item_ids {
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
        }

        if let Some(memory_group_id) = unit.memory_group_id {
            sqlx::query(
                "
                UPDATE Memories
                SET status = 'PROCESSED',
                    content_hash = ?1,
                    relative_path = ?2,
                    thumbnail_path = ?3
                WHERE id = ?4
                ",
            )
                .bind(&processed_output.content_hash)
                .bind(&relative_media_path)
                .bind(&relative_thumbnail_path)
                .bind(memory_group_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    format!(
                        "failed to update processed metadata for memory group {}: {error}",
                        memory_group_id
                    )
                })?;
        }

        window
            .emit(
                PROCESS_PROGRESS_EVENT,
                ProcessProgressPayload {
                    total_files,
                    completed_files: index + 1,
                    successful_files: processed_count,
                    failed_files: failed_count,
                    memory_item_id: Some(unit.progress_item_id),
                    status: "success".to_string(),
                    error_code: None,
                    error_message: None,
                },
            )
            .map_err(|error| format!("failed to emit processing progress event: {error}"))?;
    }

    eprintln!(
        "[processor-debug] process_downloaded_memories complete processed_count={} failed_count={} total_units={}",
        processed_count, failed_count, total_files
    );

    emit_session_log(
        &window,
        format!(
            "[{}] Processing phase finished: {} processed, {} failed",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            processed_count,
            failed_count
        ),
    )?;

    pool.close().await;
    Ok(ProcessMemoriesResult {
        processed_count,
        failed_count,
    })
}

#[tauri::command]
async fn process_memories_from_zip_archives(
    app: tauri::AppHandle,
    window: tauri::Window,
    zip_paths: Vec<String>,
    output_dir: String,
    keep_originals: bool,
) -> Result<ProcessMemoriesResult, String> {
    if zip_paths.is_empty() {
        return Err("zip_paths must not be empty".to_string());
    }

    let resolved_output_dir = resolve_output_dir(&app, &output_dir)?;
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let zip_paths = zip_paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();

    let rows = sqlx::query(
        "
        SELECT
            m.id AS memory_group_id,
            m.date AS memory_date,
            m.mid AS memory_mid,
            mi.id AS memory_item_id,
            mi.location AS location,
            mc.url AS media_url
        FROM Memories m
        JOIN MediaChunks mc
          ON mc.memory_id = m.id
         AND mc.order_index = 1
        JOIN MemoryItem mi
          ON mi.media_url = mc.url
         AND IFNULL(mi.overlay_url, '') = IFNULL(mc.overlay_url, '')
        WHERE m.mid IS NOT NULL
          AND TRIM(m.mid) != ''
          AND m.status IN ('queued', 'PENDING', 'FAILED_NETWORK')
        ORDER BY m.id ASC
        ",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load zip-process memories: {error}"))?;

    let total_files = rows.len();
    let thumbnail_path = resolved_output_dir.join(".thumbnails");
    let mut processed_count = 0usize;
    let mut failed_count = 0usize;

    emit_session_log(
        &window,
        format!(
            "[{}] ZIP-first processing started with {} archive(s)",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            zip_paths.len()
        ),
    )?;

    for (index, row) in rows.iter().enumerate() {
        let memory_group_id = row.get::<i64, _>("memory_group_id");
        let memory_item_id = row.get::<i64, _>("memory_item_id");
        let memory_date = row.get::<String, _>("memory_date");
        let memory_mid = row.get::<String, _>("memory_mid");
        let media_url = row.get::<String, _>("media_url");
        let location = row.get::<Option<String>, _>("location");

        if wait_for_pause_or_stop_and_mark_pending(
            &pool,
            &[ProcessUnit {
                memory_group_id: Some(memory_group_id),
                progress_item_id: memory_item_id,
                memory_item_ids: vec![memory_item_id],
                date_taken: memory_date.clone(),
                location: location.clone(),
            }],
        )
        .await?
        {
            break;
        }

        emit_session_log(
            &window,
            format!(
                "[{}] Extracting mid {} from ZIP archives ({}/{})",
                memory_date,
                memory_mid.chars().take(8).collect::<String>(),
                index + 1,
                total_files
            ),
        )?;

        let zip_scan = match crate::core::zip_hunter::find_and_extract_memory(
            &zip_paths,
            &memory_date,
            &memory_mid,
            Some(&media_url),
            Some(&database_url),
        )
        .await
        {
            Ok(scan) => scan,
            Err(error) => {
                failed_count += 1;

                sqlx::query(
                    "
                    UPDATE MemoryItem
                    SET status = 'processing_failed',
                        last_error_code = 'ZIP_HUNTER_FAILED',
                        last_error_message = ?1
                    WHERE id = ?2
                    ",
                )
                .bind(error.to_string())
                .bind(memory_item_id)
                .execute(&pool)
                .await
                .map_err(|db_error| format!("failed to update ZIP_HUNTER_FAILED status: {db_error}"))?;

                sqlx::query("UPDATE Memories SET status = 'FAILED_NETWORK' WHERE id = ?1")
                    .bind(memory_group_id)
                    .execute(&pool)
                    .await
                    .map_err(|db_error| format!("failed to update FAILED_NETWORK memory status: {db_error}"))?;

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
                            error_code: Some(ProcessErrorCode::ProcessingFailed),
                            error_message: Some(error.to_string()),
                        },
                    )
                    .map_err(|emit_error| format!("failed to emit zip-process error progress: {emit_error}"))?;

                continue;
            }
        };

        let active_zip_name = zip_scan
            .main_entry
            .as_ref()
            .or(zip_scan.overlay_entry.as_ref())
            .and_then(|entry| entry.zip_path.file_name())
            .and_then(|value| value.to_str())
            .map(str::to_string);

        if let Some(active_zip_name) = active_zip_name.as_deref() {
            sqlx::query("UPDATE ProcessedZips SET status = 'pending' WHERE status = 'processing'")
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to reset active zip status: {error}"))?;

            sqlx::query(
                "UPDATE ProcessedZips SET status = 'processing' WHERE filename = ?1 AND job_id = (SELECT id FROM ExportJobs ORDER BY datetime(created_at) DESC, id DESC LIMIT 1)",
            )
            .bind(active_zip_name)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to set active zip status: {error}"))?;

            emit_session_log(
                &window,
                format!(
                    "[{}] Using ZIP {} for mid {}",
                    memory_date,
                    active_zip_name,
                    memory_mid.chars().take(8).collect::<String>()
                ),
            )?;
        }

        let raw_media_paths = zip_scan
            .staged_main_path
            .clone()
            .map(|path| vec![path])
            .ok_or_else(|| "zip hunter did not produce staged main path".to_string())?;

        let process_result = crate::core::processor::process_media(crate::core::processor::ProcessMediaInput {
            memory_item_id,
            memory_group_id: Some(memory_group_id),
            raw_media_paths,
            overlay_path: zip_scan.staged_overlay_path.clone(),
            date_taken: memory_date.clone(),
            location: location.clone(),
            export_dir: resolved_output_dir.clone(),
            thumbnail_dir: thumbnail_path.clone(),
            keep_originals,
            database_url: database_url.clone(),
        })
        .await;

        match process_result {
            Ok(crate::core::processor::ProcessMediaResult::Duplicate { content_hash: _ }) => {
                sqlx::query(
                    "UPDATE MemoryItem SET status = 'duplicate', last_error_code = NULL, last_error_message = NULL WHERE id = ?1",
                )
                .bind(memory_item_id)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to update duplicate memory item: {error}"))?;

                sqlx::query("UPDATE Memories SET status = 'DUPLICATE' WHERE id = ?1")
                    .bind(memory_group_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to update duplicate memory group: {error}"))?;

                window.emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(memory_item_id),
                        status: "duplicate".to_string(),
                        error_code: None,
                        error_message: None,
                    },
                ).map_err(|error| format!("failed to emit duplicate progress: {error}"))?;
            }
            Ok(crate::core::processor::ProcessMediaResult::Processed(output)) => {
                processed_count += 1;

                let relative_media_path = path_to_relative_string(&output.final_media_path, &resolved_output_dir)?;
                let relative_thumbnail_path = path_to_relative_string(&output.thumbnail_path, &resolved_output_dir)?;

                sqlx::query(
                    "UPDATE MemoryItem SET status = 'processed', last_error_code = NULL, last_error_message = NULL WHERE id = ?1",
                )
                .bind(memory_item_id)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to update processed memory item: {error}"))?;

                sqlx::query(
                    "UPDATE Memories SET status = 'PROCESSED', content_hash = ?1, relative_path = ?2, thumbnail_path = ?3 WHERE id = ?4",
                )
                .bind(&output.content_hash)
                .bind(&relative_media_path)
                .bind(&relative_thumbnail_path)
                .bind(memory_group_id)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to update processed memory group: {error}"))?;

                if let Some(active_zip_name) = active_zip_name.as_deref() {
                    sqlx::query(
                        "UPDATE ProcessedZips SET status = 'finished' WHERE filename = ?1 AND job_id = (SELECT id FROM ExportJobs ORDER BY datetime(created_at) DESC, id DESC LIMIT 1)",
                    )
                    .bind(active_zip_name)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to update finished zip status: {error}"))?;
                }

                window.emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(memory_item_id),
                        status: "success".to_string(),
                        error_code: None,
                        error_message: None,
                    },
                ).map_err(|error| format!("failed to emit zip-process success progress: {error}"))?;
            }
            Err(error) => {
                failed_count += 1;

                sqlx::query(
                    "UPDATE MemoryItem SET status = 'processing_failed', last_error_code = 'PROCESSING_FAILED', last_error_message = ?1 WHERE id = ?2",
                )
                .bind(error.to_string())
                .bind(memory_item_id)
                .execute(&pool)
                .await
                .map_err(|db_error| format!("failed to update processing_failed memory item: {db_error}"))?;

                sqlx::query("UPDATE Memories SET status = 'processing_failed' WHERE id = ?1")
                    .bind(memory_group_id)
                    .execute(&pool)
                    .await
                    .map_err(|db_error| format!("failed to update processing_failed memory group: {db_error}"))?;

                window.emit(
                    PROCESS_PROGRESS_EVENT,
                    ProcessProgressPayload {
                        total_files,
                        completed_files: index + 1,
                        successful_files: processed_count,
                        failed_files: failed_count,
                        memory_item_id: Some(memory_item_id),
                        status: "error".to_string(),
                        error_code: Some(ProcessErrorCode::ProcessingFailed),
                        error_message: Some(error.to_string()),
                    },
                ).map_err(|emit_error| format!("failed to emit zip-process failure progress: {emit_error}"))?;
            }
        }
    }

    pool.close().await;
    Ok(ProcessMemoriesResult {
        processed_count,
        failed_count,
    })
}

async fn wait_for_pause_or_stop_and_mark_pending(
    pool: &sqlx::SqlitePool,
    remaining_units: &[ProcessUnit],
) -> Result<bool, String> {
    loop {
        let state = core::state::snapshot();

        if state.is_stopped {
            mark_remaining_units_pending(pool, remaining_units).await?;
            return Ok(true);
        }

        if !state.is_paused {
            return Ok(false);
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

async fn mark_remaining_units_pending(
    pool: &sqlx::SqlitePool,
    remaining_units: &[ProcessUnit],
) -> Result<(), String> {
    for unit in remaining_units {
        for memory_item_id in &unit.memory_item_ids {
            sqlx::query(
                "
                UPDATE MemoryItem
                SET status = 'PENDING',
                    last_error_code = NULL,
                    last_error_message = NULL
                WHERE id = ?1
                ",
            )
            .bind(memory_item_id)
            .execute(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to mark memory item {} as pending: {error}",
                    memory_item_id
                )
            })?;
        }

        if let Some(memory_group_id) = unit.memory_group_id {
            sqlx::query("UPDATE Memories SET status = 'PENDING' WHERE id = ?1")
                .bind(memory_group_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!(
                        "failed to mark memory group {} as pending: {error}",
                        memory_group_id
                    )
                })?;
        }
    }

    Ok(())
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

fn path_to_relative_string(path: &std::path::Path, root: &std::path::Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|error| {
        format!(
            "path '{}' is not under root '{}': {error}",
            path.display(),
            root.display()
        )
    })?;

    Ok(relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            tauri::async_runtime::block_on(setup_database(app.handle()))
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_system_locale,
            get_job_state,
            get_queued_count,
            set_job_state,
            db::db_get_export_job_state,
            db::db_get_pause_resume_flags,
            db::db_get_zip_status,
            import_memories_json,
            import_memories_from_zip,
            validate_memory_file,
            validate_memory_json_content,
            validate_base_zip_archive,
            initialize_zip_session,
            finalize_zip_session,
            set_processing_paused,
            stop_processing_session,
            resume_processing_session,
            get_processing_session_overview,
            download_queued_memories,
            resume_export_downloads,
            apply_metadata_to_output_files,
            process_memories_from_zip_archives,
            process_downloaded_memories,
            get_thumbnails,
            reset_all_app_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        mark_remaining_units_pending, path_to_relative_string, resolve_failed_memory_status,
        wait_for_pause_or_stop_and_mark_pending, ProcessUnit,
    };
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

    #[test]
    fn converts_output_paths_to_forward_slash_relative_strings() {
        let root = Path::new("/tmp/export");
        let path = Path::new("/tmp/export/2026/02_February/42.jpg");

        let relative = path_to_relative_string(path, root).unwrap();

        assert_eq!(relative, "2026/02_February/42.jpg");
    }

    #[test]
    fn rejects_paths_outside_output_root() {
        let root = Path::new("/tmp/export");
        let path = Path::new("/tmp/other/42.jpg");

        let error = path_to_relative_string(path, root).unwrap_err();

        assert!(error.contains("is not under root"));
    }

    #[tokio::test]
    async fn marks_remaining_units_as_pending_when_stop_is_requested() {
        crate::core::state::reset();

        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite should open");

        sqlx::query(
            "
            CREATE TABLE MemoryItem (
                id INTEGER PRIMARY KEY,
                status TEXT,
                last_error_code TEXT,
                last_error_message TEXT
            )
            ",
        )
        .execute(&pool)
        .await
        .expect("MemoryItem table should be created");

        sqlx::query(
            "
            CREATE TABLE Memories (
                id INTEGER PRIMARY KEY,
                status TEXT
            )
            ",
        )
        .execute(&pool)
        .await
        .expect("Memories table should be created");

        sqlx::query("INSERT INTO MemoryItem (id, status) VALUES (1, 'processed')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO MemoryItem (id, status) VALUES (2, 'downloaded')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO Memories (id, status) VALUES (100, 'processed')")
            .execute(&pool)
            .await
            .unwrap();

        let units = vec![ProcessUnit {
            memory_group_id: Some(100),
            progress_item_id: 2,
            memory_item_ids: vec![2],
            date_taken: "2026-02-20".to_string(),
            location: None,
        }];

        crate::core::state::set_stopped(true);
        let should_stop = wait_for_pause_or_stop_and_mark_pending(&pool, &units)
            .await
            .expect("stop handling should succeed");

        assert!(should_stop);

        let item_status: String = sqlx::query_scalar("SELECT status FROM MemoryItem WHERE id = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
        let group_status: String = sqlx::query_scalar("SELECT status FROM Memories WHERE id = 100")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(item_status, "PENDING");
        assert_eq!(group_status, "PENDING");

        crate::core::state::reset();
    }

    #[tokio::test]
    async fn mark_remaining_units_pending_updates_all_related_rows() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite should open");

        sqlx::query(
            "
            CREATE TABLE MemoryItem (
                id INTEGER PRIMARY KEY,
                status TEXT,
                last_error_code TEXT,
                last_error_message TEXT
            )
            ",
        )
        .execute(&pool)
        .await
        .expect("MemoryItem table should be created");

        sqlx::query(
            "
            CREATE TABLE Memories (
                id INTEGER PRIMARY KEY,
                status TEXT
            )
            ",
        )
        .execute(&pool)
        .await
        .expect("Memories table should be created");

        sqlx::query("INSERT INTO MemoryItem (id, status) VALUES (5, 'processing_failed')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO MemoryItem (id, status) VALUES (6, 'downloaded')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO Memories (id, status) VALUES (200, 'processing_failed')")
            .execute(&pool)
            .await
            .unwrap();

        let units = vec![ProcessUnit {
            memory_group_id: Some(200),
            progress_item_id: 5,
            memory_item_ids: vec![5, 6],
            date_taken: "2026-02-20".to_string(),
            location: None,
        }];

        mark_remaining_units_pending(&pool, &units)
            .await
            .expect("pending update should succeed");

        let statuses: Vec<String> = sqlx::query_scalar(
            "SELECT status FROM MemoryItem WHERE id IN (5, 6) ORDER BY id ASC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let group_status: String = sqlx::query_scalar("SELECT status FROM Memories WHERE id = 200")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(statuses, vec!["PENDING".to_string(), "PENDING".to_string()]);
        assert_eq!(group_status, "PENDING");
    }
}
