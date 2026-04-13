pub mod core;
pub mod db;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Row;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

const VIEWER_ARCHIVE_MANIFEST_NAME: &str = "viewer_manifest.json";
const APP_CONFIG_FILENAME: &str = "config.json";

const MAX_PERSISTED_RETRY_ATTEMPTS: i64 = 3;
const PROCESS_PROGRESS_EVENT: &str = "process-progress";
const SESSION_LOG_EVENT: &str = "session-log";
const ZIP_HUNTER_TIMEOUT_SECS: u64 = 60;
const PROCESS_MEDIA_TIMEOUT_SECS: u64 = 60;

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
    missing_files: i64,
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
    missing_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MissingFileItem {
    memory_group_id: i64,
    memory_item_id: i64,
    date_taken: String,
    mid: Option<String>,
    location: Option<String>,
    media_download_url: String,
    last_error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveCreationResult {
    archive_path: String,
    added_files: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewerArchiveImportResult {
    imported_count: usize,
    skipped_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewerArchiveManifest {
    archive_type: String,
    version: u8,
    created_at: String,
}

#[derive(Debug, Clone)]
struct ProcessUnit {
    memory_group_id: Option<i64>,
    progress_item_id: i64,
    memory_item_ids: Vec<i64>,
    date_taken: String,
    location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AppConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    export_path: Option<String>,
}

struct AppConfigState(Mutex<AppConfig>);

fn app_config_file_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|error| format!("failed to create app data directory: {error}"))?;
    Ok(app_data_dir.join(APP_CONFIG_FILENAME))
}

fn load_app_config(app: &tauri::AppHandle) -> AppConfig {
    let config_path = match app_config_file_path(app) {
        Ok(path) => path,
        Err(_) => return AppConfig::default(),
    };
    if !config_path.exists() {
        return AppConfig::default();
    }
    match std::fs::read_to_string(&config_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

fn save_app_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let config_path = app_config_file_path(app)?;
    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("failed to serialize app config: {error}"))?;
    std::fs::write(&config_path, content)
        .map_err(|error| format!("failed to write app config: {error}"))?;
    Ok(())
}

fn resolve_base_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let config_state = app.state::<AppConfigState>();
    let config = config_state
        .0
        .lock()
        .map_err(|error| format!("config lock poisoned: {error}"))?;
    if let Some(ref custom_path) = config.export_path {
        let path = std::path::PathBuf::from(custom_path);
        if path.is_absolute() {
            return Ok(path);
        }
    }
    app.path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskSpaceInfo {
    total_bytes: u64,
    free_bytes: u64,
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
    debug_stage: Option<String>,
    debug_mid: Option<String>,
    debug_date: Option<String>,
    debug_zip: Option<String>,
    debug_details: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewerItem {
    memory_item_id: i64,
    date_taken: String,
    location: Option<String>,
    raw_location: Option<String>,
    thumbnail_path: String,
    media_path: String,
    media_kind: ViewerMediaKind,
    media_format: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum ViewerMediaKind {
    Image,
    Video,
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

fn truncate_debug_text(value: &str, max_chars: usize) -> String {
    let mut characters = value.chars();
    let truncated: String = characters.by_ref().take(max_chars).collect();

    if characters.next().is_some() {
        format!("{truncated}...[truncated]")
    } else {
        truncated
    }
}

fn describe_process_error(error: &core::processor::ProcessorError) -> (String, String) {
    use crate::core::media::MediaError;
    use crate::core::processor::ProcessorError;

    match error {
        ProcessorError::Media(media_error) => match media_error {
            MediaError::FfmpegFailed { status, stderr } => (
                "media.ffmpeg".to_string(),
                format!(
                    "ffmpeg failed (status={status:?}): {}",
                    truncate_debug_text(stderr, 3200)
                ),
            ),
            MediaError::MissingOverlay(path) => (
                "media.overlay_missing".to_string(),
                format!("overlay file is missing: {}", path.display()),
            ),
            MediaError::UnsupportedMediaType(path) => (
                "media.unsupported_type".to_string(),
                format!("unsupported media type: {}", path.display()),
            ),
            MediaError::InvalidMetadata(reason) => (
                "media.invalid_metadata".to_string(),
                truncate_debug_text(reason, 1200),
            ),
            MediaError::Io(io_error) => ("media.io".to_string(), io_error.to_string()),
            MediaError::Join(join_error) => ("media.join".to_string(), join_error.to_string()),
        },
        ProcessorError::FfmpegFailed { status, stderr } => (
            "processor.ffmpeg".to_string(),
            format!(
                "ffmpeg failed (status={status:?}): {}",
                truncate_debug_text(stderr, 3200)
            ),
        ),
        ProcessorError::Io(io_error) => ("processor.io".to_string(), io_error.to_string()),
        ProcessorError::Join(join_error) => ("processor.join".to_string(), join_error.to_string()),
        ProcessorError::Database(db_error) => ("processor.db".to_string(), db_error.to_string()),
        ProcessorError::InvalidInput(reason) => {
            ("processor.invalid_input".to_string(), reason.to_string())
        }
        ProcessorError::Blake3(reason) => (
            "processor.blake3".to_string(),
            truncate_debug_text(reason, 1200),
        ),
    }
}

fn is_snapchat_export_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn parse_snapchat_zip_name(file_stem: &str) -> Result<(String, Option<u32>), String> {
    let prefix = "mydata~";
    if !file_stem.starts_with(prefix) {
        return Err(format!("zip '{file_stem}' must start with '{prefix}'"));
    }

    let rest = &file_stem[prefix.len()..];
    if rest.is_empty() {
        return Err(format!("zip '{file_stem}' is missing uuid segment"));
    }

    if let Some((uuid_candidate, number_candidate)) = rest.rsplit_once('-') {
        if number_candidate
            .chars()
            .all(|character| character.is_ascii_digit())
            && is_snapchat_export_id(uuid_candidate)
        {
            let parsed_number = number_candidate
                .parse::<u32>()
                .map_err(|error| format!("invalid zip part number in '{file_stem}': {error}"))?;
            return Ok((uuid_candidate.to_string(), Some(parsed_number)));
        }
    }

    if is_snapchat_export_id(rest) {
        return Ok((rest.to_string(), None));
    }

    Err(format!(
        "zip '{file_stem}' must use 'mydata~<uuid>.zip' or 'mydata~<uuid>-<num>.zip' naming"
    ))
}

fn zip_contains_required_memories_entry(zip_path: &std::path::Path) -> Result<bool, String> {
    let file = std::fs::File::open(zip_path).map_err(|error| {
        format!(
            "failed to open zip '{}' for base verification: {error}",
            zip_path.display()
        )
    })?;

    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        format!(
            "failed to read zip archive '{}': {error}",
            zip_path.display()
        )
    })?;

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

    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        format!(
            "failed to read zip archive '{}': {error}",
            zip_path.display()
        )
    })?;

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
    let mut base_dir = resolve_base_dir(app)?;

    std::fs::create_dir_all(&base_dir)
        .map_err(|error| format!("failed to create base directory: {error}"))?;

    base_dir.push("memories.db");

    Ok(core::sqlite_url_from_path(&base_dir))
}

fn resolve_output_dir(
    app: &tauri::AppHandle,
    output_dir: &str,
) -> Result<std::path::PathBuf, String> {
    let requested_path = std::path::PathBuf::from(output_dir);
    if requested_path.is_absolute() {
        return Ok(requested_path);
    }

    let mut base_dir = resolve_base_dir(app)?;

    std::fs::create_dir_all(&base_dir)
        .map_err(|error| format!("failed to create base directory for output dir: {error}"))?;

    base_dir.push(requested_path);
    Ok(base_dir)
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
            format!("failed to add column {column_name} to table {table_name}: {error}")
        })?;

    Ok(())
}

async fn setup_database(app: &tauri::AppHandle) -> Result<(), String> {
    let mut base_dir = resolve_base_dir(app)?;

    std::fs::create_dir_all(&base_dir)
        .map_err(|error| format!("failed to create base directory for database: {error}"))?;

    base_dir.push("memories.db");

    let connect_options = SqliteConnectOptions::new()
        .filename(&base_dir)
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
                media_download_url TEXT,
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

    ensure_sqlite_column(&pool, "MemoryItem", "date_time", "date_time TEXT").await?;
    ensure_sqlite_column(
        &pool,
        "MemoryItem",
        "location_resolved",
        "location_resolved TEXT",
    )
    .await?;
    ensure_sqlite_column(
        &pool,
        "MemoryItem",
        "media_download_url",
        "media_download_url TEXT",
    )
    .await?;

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
fn probe_system_codecs() -> core::media::SystemCodecInfo {
    core::media::probe_system_codecs()
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
        SELECT
            id,
            COALESCE(NULLIF(TRIM(media_download_url), ''), media_url) AS media_download_url,
            overlay_url,
            retry_count
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
        VALUES (1, 'downloading', ?1, 0)
        ON CONFLICT(id) DO UPDATE SET
            status = 'downloading',
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
            let media_download_url = row.get::<String, _>("media_download_url");
            let overlay_url = row.get::<Option<String>, _>("overlay_url");
            let retry_count = row.get::<i64, _>("retry_count");

            retry_counts_by_id.insert(id, retry_count);
            overlay_urls_by_id.insert(id, overlay_url);

            let extension = extract_extension_from_url(&media_download_url, "bin");

            core::downloader::DownloadTask {
                memory_item_id: id,
                url: media_download_url,
                destination_path: resolved_output_dir.join(format!("{id}.{extension}")),
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

    let download_results =
        core::downloader::download_tasks_with_progress_and_rate_limits(&window, tasks, rate_limits)
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
                emit_session_log(
                    &window,
                    format!(
                        "[{}] Downloaded memory item {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        download_result.memory_item_id
                    ),
                )?;

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
                if error.is_stopped() {
                    emit_session_log(
                        &window,
                        format!(
                            "[{}] Download paused/stopped by user",
                            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                        ),
                    )?;
                    break;
                }

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
            if find_overlay_file_for_memory_item(&resolved_output_dir, *memory_item_id)
                .ok()
                .flatten()
                .is_some()
            {
                return None;
            }

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
    } else if core::state::snapshot().is_stopped {
        "stopped"
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

    let parsed_zip_paths: Vec<std::path::PathBuf> =
        zip_paths.iter().map(std::path::PathBuf::from).collect();

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
                    return Err(
                        "multiple main zip files detected; provide exactly one mydata~<uuid>.zip"
                            .to_string(),
                    );
                }
                main_zip = Some((zip_path.clone(), uuid));
            }
            Some(part_number) => {
                optional_part_zips.push((zip_path.clone(), uuid, part_number));
            }
        }
    }

    let (main_zip_path, main_uuid) = main_zip.ok_or_else(|| {
        "missing main zip file; provide mydata~<uuid>.zip as the base archive".to_string()
    })?;

    if !zip_contains_required_memories_entry(&main_zip_path)? {
        return Err(format!(
            "first zip '{}' must contain json/memories_history.json",
            main_zip_path.display()
        ));
    }

    for (_, uuid, _) in &optional_part_zips {
        if uuid != &main_uuid {
            return Err(
                "all optional zip parts must belong to the same mydata~<uuid> set as the main zip"
                    .to_string(),
            );
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

        core::state::set_stopped(true);
        core::state::set_paused(false);
    }

    pool.close().await;
    Ok(())
}

#[tauri::command]
async fn get_missing_files(app: tauri::AppHandle) -> Result<Vec<MissingFileItem>, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let rows = sqlx::query(
        "
        SELECT
            m.id AS memory_group_id,
            mi.id AS memory_item_id,
            m.date AS date_taken,
            m.mid AS mid,
            COALESCE(mi.location_resolved, mi.location) AS location,
            COALESCE(NULLIF(TRIM(mi.media_download_url), ''), mi.media_url) AS media_download_url,
            mi.last_error_message AS last_error_message
        FROM Memories m
        JOIN MediaChunks mc
          ON mc.memory_id = m.id
         AND mc.order_index = 1
        JOIN MemoryItem mi
          ON mi.id = (
                SELECT mi2.id
                FROM MemoryItem mi2
                WHERE mi2.media_url = mc.url
                  AND IFNULL(mi2.overlay_url, '') = IFNULL(mc.overlay_url, '')
                ORDER BY mi2.id ASC
                LIMIT 1
             )
        WHERE m.status = 'MISSING_LOCAL'
        ORDER BY m.id ASC
        ",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load missing files list: {error}"))?;

    pool.close().await;

    Ok(rows
        .into_iter()
        .map(|row| MissingFileItem {
            memory_group_id: row.get::<i64, _>("memory_group_id"),
            memory_item_id: row.get::<i64, _>("memory_item_id"),
            date_taken: row.get::<String, _>("date_taken"),
            mid: row.get::<Option<String>, _>("mid"),
            location: row.get::<Option<String>, _>("location"),
            media_download_url: row.get::<String, _>("media_download_url"),
            last_error_message: row.get::<Option<String>, _>("last_error_message"),
        })
        .collect())
}

#[tauri::command]
async fn get_missing_file_by_memory_item_id(
    app: tauri::AppHandle,
    memory_item_id: i64,
) -> Result<Option<MissingFileItem>, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let row = sqlx::query(
        "
        SELECT
            m.id AS memory_group_id,
            mi.id AS memory_item_id,
            m.date AS date_taken,
            m.mid AS mid,
            COALESCE(mi.location_resolved, mi.location) AS location,
            COALESCE(NULLIF(TRIM(mi.media_download_url), ''), mi.media_url) AS media_download_url,
            mi.last_error_message AS last_error_message
        FROM MemoryItem mi
        JOIN MediaChunks mc
          ON mc.url = mi.media_url
         AND IFNULL(mc.overlay_url, '') = IFNULL(mi.overlay_url, '')
         AND mc.order_index = 1
        JOIN Memories m
          ON m.id = mc.memory_id
        WHERE mi.id = ?1
          AND m.status = 'MISSING_LOCAL'
        LIMIT 1
        ",
    )
    .bind(memory_item_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("failed to load missing file item by memory id: {error}"))?;

    pool.close().await;

    Ok(row.map(|row| MissingFileItem {
        memory_group_id: row.get::<i64, _>("memory_group_id"),
        memory_item_id: row.get::<i64, _>("memory_item_id"),
        date_taken: row.get::<String, _>("date_taken"),
        mid: row.get::<Option<String>, _>("mid"),
        location: row.get::<Option<String>, _>("location"),
        media_download_url: row.get::<String, _>("media_download_url"),
        last_error_message: row.get::<Option<String>, _>("last_error_message"),
    }))
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

    let processed_files =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM MemoryItem WHERE status = 'processed'")
            .fetch_one(&pool)
            .await
            .map_err(|error| format!("failed to read processed files count: {error}"))?;

    let missing_files = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM Memories WHERE status = 'MISSING_LOCAL'",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("failed to read missing files count: {error}"))?;

    let duplicates_skipped =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM MemoryItem WHERE status = 'duplicate'")
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
        missing_files,
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
        "SELECT id, date, COALESCE(location_resolved, location) AS location FROM MemoryItem WHERE status IN ('downloaded', 'processed') ORDER BY id ASC",
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

            let resolved_thumbnail_path =
                std::fs::canonicalize(&thumbnail_path).unwrap_or(thumbnail_path);

            Some(ThumbnailItem {
                memory_item_id,
                thumbnail_path: resolved_thumbnail_path.to_string_lossy().to_string(),
            })
        })
        .collect::<Vec<_>>();

    Ok(items)
}

#[tauri::command]
async fn get_viewer_items(
    app: tauri::AppHandle,
    offset: i64,
    limit: i64,
) -> Result<Vec<ViewerItem>, String> {
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
                SELECT mi.id,
                             COALESCE(mi.date_time, mi.date) AS date,
                             mi.location_resolved,
                             mi.location,
                             m.relative_path,
                             m.thumbnail_path
                FROM MemoryItem mi
                LEFT JOIN MediaChunks mc
                    ON mc.url = mi.media_url
                 AND IFNULL(mc.overlay_url, '') = IFNULL(mi.overlay_url, '')
                 AND mc.order_index = 1
                LEFT JOIN Memories m
                    ON m.id = mc.memory_id
                WHERE mi.status = 'processed'
        ORDER BY CASE
                                         WHEN mi.date_time IS NOT NULL AND TRIM(mi.date_time) <> '' THEN datetime(mi.date_time)
                                         WHEN mi.date IS NOT NULL AND TRIM(mi.date) <> '' THEN datetime(mi.date)
                     ELSE NULL
                 END DESC,
                                 mi.id DESC
        LIMIT ?1 OFFSET ?2
        ",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to query processed memories for viewer items: {error}"))?;

    let output_dir = resolve_output_dir(&app, ".raw_cache")?;
    let thumbnails_dir = output_dir.join(".thumbnails");

    let mut items = Vec::new();

    for row in rows {
        let memory_item_id = row.get::<i64, _>("id");
        let date_taken = row.get::<String, _>("date");
        let location_resolved: Option<String> = row.try_get("location_resolved").ok().flatten();
        let location_raw: Option<String> = row.try_get("location").ok().flatten();
        let relative_media_path: Option<String> = row.try_get("relative_path").ok().flatten();
        let relative_thumbnail_path: Option<String> = row.try_get("thumbnail_path").ok().flatten();
        let normalized_raw_location = location_raw
            .as_deref()
            .and_then(crate::core::geocoder::normalize_location_text);
        let location = location_resolved
            .clone()
            .or_else(|| normalized_raw_location.clone());
        let raw_location =
            normalized_raw_location.filter(|raw| location.as_deref() != Some(raw.as_str()));

        let thumbnail_path = relative_thumbnail_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(|path| output_dir.join(path))
            .unwrap_or_else(|| thumbnails_dir.join(format!("{memory_item_id}.webp")));
        if !thumbnail_path.exists() {
            continue;
        }

        let media_path = match relative_media_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(|path| output_dir.join(path))
            .filter(|path| path.exists())
        {
            Some(path) => path,
            None => match find_output_file_for_memory_item_recursive(&output_dir, memory_item_id) {
                Ok(Some(path)) => path,
                Ok(None) => continue,
                Err(error) => {
                    eprintln!(
                        "[viewer] failed to locate media path for memory item {}: {}",
                        memory_item_id, error
                    );
                    continue;
                }
            },
        };

        let Some(media_kind) = viewer_media_kind_from_path(&media_path) else {
            continue;
        };
        let media_format = media_path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_uppercase());

        items.push(ViewerItem {
            memory_item_id,
            date_taken,
            location,
            raw_location,
            thumbnail_path: thumbnail_path.to_string_lossy().to_string(),
            media_path: media_path.to_string_lossy().to_string(),
            media_kind,
            media_format,
        });
    }

    pool.close().await;

    Ok(items)
}

#[tauri::command]
async fn has_viewer_items(app: tauri::AppHandle) -> Result<bool, String> {
    let database_url = memories_db_url(&app)?;
    let pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .map_err(|error| format!("failed to connect to memories database: {error}"))?;

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM MemoryItem WHERE status = 'processed' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("failed to query processed memories count: {error}"))?;

    pool.close().await;
    Ok(count > 0)
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
#[allow(clippy::too_many_arguments)]
async fn process_downloaded_memories(
    app: tauri::AppHandle,
    window: tauri::Window,
    output_dir: String,
    keep_originals: bool,
    thumbnail_quality: Option<String>,
    video_profile: Option<String>,
    image_output_format: Option<String>,
    image_quality: Option<String>,
    encoding_hw_accel: Option<String>,
    overlay_strategy: Option<String>,
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
    let thumbnail_max_dimension =
        core::processor::ThumbnailQuality::from_setting(thumbnail_quality.as_deref())
            .max_dimension();
    let video_output_profile =
        core::media::VideoOutputProfile::from_setting(video_profile.as_deref());
    let image_output_format =
        core::media::ImageOutputFormat::from_setting(image_output_format.as_deref());
    let image_quality = core::media::ImageQuality::from_setting(image_quality.as_deref());
    let hw_accel = core::media::HwAccelPreference::from_setting(encoding_hw_accel.as_deref());
    let overlay_strat = core::media::OverlayStrategy::from_setting(overlay_strategy.as_deref());
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
            COALESCE(mi.location_resolved, mi.location) AS location
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
    .map_err(|error| {
        format!("failed to load grouped downloaded memories for processing: {error}")
    })?;

    let process_units: Vec<ProcessUnit> = if chunk_rows.is_empty() {
        eprintln!(
            "[processor-debug] no MediaChunks groups found, falling back to per-item processing"
        );

        let fallback_rows = sqlx::query(
            "
            SELECT id, date, COALESCE(location_resolved, location) AS location
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

            let entry = grouped_units
                .entry(memory_group_id)
                .or_insert_with(|| ProcessUnit {
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
                        debug_stage: Some("download.lookup".to_string()),
                        debug_mid: None,
                        debug_date: Some(unit.date_taken.clone()),
                        debug_zip: None,
                        debug_details: Some(format!(
                            "missing downloaded file for memory_item_id={missing_memory_item_id}"
                        )),
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
            thumbnail_max_dimension,
            video_output_profile,
            image_output_format,
            image_quality,
            hw_accel,
            overlay_strategy: overlay_strat,
            keep_originals,
            database_url: database_url.clone(),
        })
        .await;

        if let Err(error) = process_result {
            failed_count += 1;
            let (debug_stage, debug_details) = describe_process_error(&error);
            let concise_error = format!("processing failed at stage {debug_stage}");

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
                        error_message: Some(concise_error),
                        debug_stage: Some(debug_stage),
                        debug_mid: None,
                        debug_date: Some(unit.date_taken.clone()),
                        debug_zip: None,
                        debug_details: Some(debug_details),
                    },
                )
                .map_err(|emit_error| {
                    format!("failed to emit processing progress event: {emit_error}")
                })?;
            continue;
        }

        if let Ok(core::processor::ProcessMediaResult::Duplicate { ref content_hash }) =
            process_result
        {
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
                        debug_stage: Some("process.duplicate".to_string()),
                        debug_mid: None,
                        debug_date: Some(unit.date_taken.clone()),
                        debug_zip: None,
                        debug_details: None,
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

        let relative_media_path =
            path_to_relative_string(&processed_output.final_media_path, output_path).map_err(
                |error| {
                    format!(
                        "failed to compute relative media path for memory {}: {error}",
                        unit.progress_item_id
                    )
                },
            )?;
        let relative_thumbnail_path =
            path_to_relative_string(&processed_output.thumbnail_path, output_path).map_err(
                |error| {
                    format!(
                        "failed to compute relative thumbnail path for memory {}: {error}",
                        unit.progress_item_id
                    )
                },
            )?;

        let (success_stage, success_details): (String, Option<String>) =
            if processed_output.overlay_requested && processed_output.overlay_applied {
                ("process.success.overlay_applied".to_string(), None)
            } else if processed_output.overlay_requested && !processed_output.overlay_applied {
                (
                    "process.success.overlay_fallback".to_string(),
                    processed_output
                        .overlay_fallback_reason
                        .as_deref()
                        .map(|reason| truncate_debug_text(reason, 3200)),
                )
            } else {
                ("process.success.no_overlay".to_string(), None)
            };

        if success_stage == "process.success.overlay_fallback" {
            let fallback_reason = success_details
                .clone()
                .unwrap_or_else(|| "overlay fallback without explicit reason".to_string());
            emit_session_log(
                &window,
                format!(
                    "[{}] Overlay fallback used for memory_item_id={} date={} reason={}",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    unit.progress_item_id,
                    unit.date_taken,
                    fallback_reason
                ),
            )?;
        }

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
                    debug_stage: Some(success_stage),
                    debug_mid: None,
                    debug_date: Some(unit.date_taken.clone()),
                    debug_zip: None,
                    debug_details: success_details,
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
        missing_count: 0,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn process_memories_from_zip_archives(
    app: tauri::AppHandle,
    window: tauri::Window,
    zip_paths: Vec<String>,
    output_dir: String,
    keep_originals: bool,
    thumbnail_quality: Option<String>,
    video_profile: Option<String>,
    image_output_format: Option<String>,
    image_quality: Option<String>,
    encoding_hw_accel: Option<String>,
    overlay_strategy: Option<String>,
) -> Result<ProcessMemoriesResult, String> {
    if zip_paths.is_empty() {
        return Err("zip_paths must not be empty".to_string());
    }

    let resolved_output_dir = resolve_output_dir(&app, &output_dir)?;
    let database_url = memories_db_url(&app)?;
    let thumbnail_max_dimension =
        crate::core::processor::ThumbnailQuality::from_setting(thumbnail_quality.as_deref())
            .max_dimension();
    let video_output_profile =
        crate::core::media::VideoOutputProfile::from_setting(video_profile.as_deref());
    let image_output_format =
        crate::core::media::ImageOutputFormat::from_setting(image_output_format.as_deref());
    let image_quality = crate::core::media::ImageQuality::from_setting(image_quality.as_deref());
    let hw_accel =
        crate::core::media::HwAccelPreference::from_setting(encoding_hw_accel.as_deref());
    let overlay_strat =
        crate::core::media::OverlayStrategy::from_setting(overlay_strategy.as_deref());
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
            COALESCE(mi.location_resolved, mi.location) AS location
        FROM Memories m
        JOIN MediaChunks mc
          ON mc.memory_id = m.id
         AND mc.order_index = 1
        JOIN MemoryItem mi
          ON mi.media_url = mc.url
         AND IFNULL(mi.overlay_url, '') = IFNULL(mc.overlay_url, '')
        WHERE m.mid IS NOT NULL
          AND TRIM(m.mid) != ''
                    AND m.status IN ('queued', 'PENDING', 'FAILED_NETWORK', 'MISSING_LOCAL')
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
    let mut missing_count = 0usize;

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

        let zip_scan = match tokio::time::timeout(
            std::time::Duration::from_secs(ZIP_HUNTER_TIMEOUT_SECS),
            crate::core::zip_hunter::find_and_extract_memory(
                &zip_paths,
                &memory_date,
                &memory_mid,
                None,
                Some(&database_url),
            ),
        )
        .await
        {
            Err(_) => {
                failed_count += 1;

                let timeout_message = format!(
                    "zip scan timed out after {}s while resolving mid {}",
                    ZIP_HUNTER_TIMEOUT_SECS,
                    memory_mid.chars().take(8).collect::<String>()
                );

                sqlx::query(
                    "
                    UPDATE MemoryItem
                    SET status = 'processing_failed',
                        last_error_code = 'ZIP_HUNTER_TIMEOUT',
                        last_error_message = ?1
                    WHERE id = ?2
                    ",
                )
                .bind(&timeout_message)
                .bind(memory_item_id)
                .execute(&pool)
                .await
                .map_err(|db_error| {
                    format!("failed to update ZIP_HUNTER_TIMEOUT status: {db_error}")
                })?;

                sqlx::query("UPDATE Memories SET status = 'FAILED_NETWORK' WHERE id = ?1")
                    .bind(memory_group_id)
                    .execute(&pool)
                    .await
                    .map_err(|db_error| {
                        format!("failed to update FAILED_NETWORK memory status: {db_error}")
                    })?;

                emit_session_log(
                    &window,
                    format!(
                        "[{}] Timeout while scanning ZIP archives for mid {} (>{}s)",
                        memory_date,
                        memory_mid.chars().take(8).collect::<String>(),
                        ZIP_HUNTER_TIMEOUT_SECS
                    ),
                )?;

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
                            error_message: Some(timeout_message),
                            debug_stage: Some("zip.scan.timeout".to_string()),
                            debug_mid: Some(memory_mid.clone()),
                            debug_date: Some(memory_date.clone()),
                            debug_zip: None,
                            debug_details: Some(
                                "zip_hunter exceeded timeout while scanning all provided archives"
                                    .to_string(),
                            ),
                        },
                    )
                    .map_err(|emit_error| {
                        format!("failed to emit zip-process timeout progress: {emit_error}")
                    })?;

                continue;
            }
            Ok(result) => match result {
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
                    .map_err(|db_error| {
                        format!("failed to update ZIP_HUNTER_FAILED status: {db_error}")
                    })?;

                    sqlx::query("UPDATE Memories SET status = 'FAILED_NETWORK' WHERE id = ?1")
                        .bind(memory_group_id)
                        .execute(&pool)
                        .await
                        .map_err(|db_error| {
                            format!("failed to update FAILED_NETWORK memory status: {db_error}")
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
                                error_code: Some(ProcessErrorCode::ProcessingFailed),
                                error_message: Some(error.to_string()),
                                debug_stage: Some("zip.scan.error".to_string()),
                                debug_mid: Some(memory_mid.clone()),
                                debug_date: Some(memory_date.clone()),
                                debug_zip: None,
                                debug_details: Some(truncate_debug_text(&error.to_string(), 3200)),
                            },
                        )
                        .map_err(|emit_error| {
                            format!("failed to emit zip-process error progress: {emit_error}")
                        })?;

                    continue;
                }
            },
        };

        if zip_scan.staged_main_path.is_none() {
            missing_count += 1;

            sqlx::query(
                "
                UPDATE MemoryItem
                SET status = 'queued',
                    last_error_code = 'MISSING_LOCAL_ARCHIVE',
                    last_error_message = 'media was not found in provided zip archives'
                WHERE id = ?1
                ",
            )
            .bind(memory_item_id)
            .execute(&pool)
            .await
            .map_err(|db_error| format!("failed to update missing local status: {db_error}"))?;

            sqlx::query("UPDATE Memories SET status = 'MISSING_LOCAL' WHERE id = ?1")
                .bind(memory_group_id)
                .execute(&pool)
                .await
                .map_err(|db_error| {
                    format!("failed to update MISSING_LOCAL memory status: {db_error}")
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
                        status: "missing".to_string(),
                        error_code: None,
                        error_message: Some(
                            "media was not found in provided zip archives".to_string(),
                        ),
                        debug_stage: Some("zip.scan.missing".to_string()),
                        debug_mid: Some(memory_mid.clone()),
                        debug_date: Some(memory_date.clone()),
                        debug_zip: None,
                        debug_details: Some(
                            "no matching <date>_<mid>-main media entry found across provided zips"
                                .to_string(),
                        ),
                    },
                )
                .map_err(|emit_error| {
                    format!("failed to emit zip-process missing progress: {emit_error}")
                })?;

            continue;
        }

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

        let process_result = match tokio::time::timeout(
            std::time::Duration::from_secs(PROCESS_MEDIA_TIMEOUT_SECS),
            crate::core::processor::process_media(crate::core::processor::ProcessMediaInput {
                memory_item_id,
                memory_group_id: Some(memory_group_id),
                raw_media_paths,
                overlay_path: zip_scan.staged_overlay_path.clone(),
                date_taken: memory_date.clone(),
                location: location.clone(),
                export_dir: resolved_output_dir.clone(),
                thumbnail_dir: thumbnail_path.clone(),
                thumbnail_max_dimension,
                video_output_profile,
                image_output_format,
                image_quality,
                hw_accel,
                overlay_strategy: overlay_strat,
                keep_originals,
                database_url: database_url.clone(),
            }),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                failed_count += 1;

                let timeout_message = format!(
                    "media processing timed out after {}s for mid {}",
                    PROCESS_MEDIA_TIMEOUT_SECS,
                    memory_mid.chars().take(8).collect::<String>()
                );

                emit_session_log(
                    &window,
                    format!(
                        "[{}] Timeout while processing mid {} (>{}s); skipping item",
                        memory_date,
                        memory_mid.chars().take(8).collect::<String>(),
                        PROCESS_MEDIA_TIMEOUT_SECS
                    ),
                )?;

                sqlx::query(
                    "UPDATE MemoryItem SET status = 'processing_failed', last_error_code = 'PROCESSING_TIMEOUT', last_error_message = ?1 WHERE id = ?2",
                )
                .bind(&timeout_message)
                .bind(memory_item_id)
                .execute(&pool)
                .await
                .map_err(|db_error| format!("failed to update processing timeout memory item: {db_error}"))?;

                sqlx::query("UPDATE Memories SET status = 'processing_failed' WHERE id = ?1")
                    .bind(memory_group_id)
                    .execute(&pool)
                    .await
                    .map_err(|db_error| {
                        format!("failed to update processing timeout memory group: {db_error}")
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
                            error_code: Some(ProcessErrorCode::ProcessingFailed),
                            error_message: Some(timeout_message),
                            debug_stage: Some("process.timeout".to_string()),
                            debug_mid: Some(memory_mid.clone()),
                            debug_date: Some(memory_date.clone()),
                            debug_zip: active_zip_name.clone(),
                            debug_details: Some(
                                "process_media exceeded timeout; likely ffmpeg/transcode blocked"
                                    .to_string(),
                            ),
                        },
                    )
                    .map_err(|emit_error| {
                        format!("failed to emit processing-timeout progress: {emit_error}")
                    })?;

                continue;
            }
        };

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

                window
                    .emit(
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
                            debug_stage: Some("process.duplicate".to_string()),
                            debug_mid: Some(memory_mid.clone()),
                            debug_date: Some(memory_date.clone()),
                            debug_zip: active_zip_name.clone(),
                            debug_details: None,
                        },
                    )
                    .map_err(|error| format!("failed to emit duplicate progress: {error}"))?;
            }
            Ok(crate::core::processor::ProcessMediaResult::Processed(output)) => {
                let (success_stage, success_details): (String, Option<String>) =
                    if output.overlay_requested && output.overlay_applied {
                        ("process.success.overlay_applied".to_string(), None)
                    } else if output.overlay_requested && !output.overlay_applied {
                        (
                            "process.success.overlay_fallback".to_string(),
                            output
                                .overlay_fallback_reason
                                .as_deref()
                                .map(|reason| truncate_debug_text(reason, 3200)),
                        )
                    } else {
                        ("process.success.no_overlay".to_string(), None)
                    };

                if success_stage == "process.success.overlay_fallback" {
                    let short_mid = memory_mid.chars().take(8).collect::<String>();
                    let zip_name = active_zip_name
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    let fallback_reason = success_details
                        .clone()
                        .unwrap_or_else(|| "overlay fallback without explicit reason".to_string());

                    emit_session_log(
                        &window,
                        format!(
                            "[{}] Overlay fallback used for date={} mid={} zip={} reason={}",
                            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                            memory_date,
                            short_mid,
                            zip_name,
                            fallback_reason
                        ),
                    )?;
                }

                processed_count += 1;

                let relative_media_path =
                    path_to_relative_string(&output.final_media_path, &resolved_output_dir)?;
                let relative_thumbnail_path =
                    path_to_relative_string(&output.thumbnail_path, &resolved_output_dir)?;

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
                            error_code: None,
                            error_message: None,
                            debug_stage: Some(success_stage),
                            debug_mid: Some(memory_mid.clone()),
                            debug_date: Some(memory_date.clone()),
                            debug_zip: active_zip_name.clone(),
                            debug_details: success_details,
                        },
                    )
                    .map_err(|error| {
                        format!("failed to emit zip-process success progress: {error}")
                    })?;
            }
            Err(error) => {
                failed_count += 1;
                let (debug_stage, debug_details) = describe_process_error(&error);
                let concise_error = format!("processing failed at stage {debug_stage}");

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
                    .map_err(|db_error| {
                        format!("failed to update processing_failed memory group: {db_error}")
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
                            error_code: Some(ProcessErrorCode::ProcessingFailed),
                            error_message: Some(concise_error),
                            debug_stage: Some(debug_stage),
                            debug_mid: Some(memory_mid.clone()),
                            debug_date: Some(memory_date.clone()),
                            debug_zip: active_zip_name.clone(),
                            debug_details: Some(debug_details),
                        },
                    )
                    .map_err(|emit_error| {
                        format!("failed to emit zip-process failure progress: {emit_error}")
                    })?;
            }
        }
    }

    pool.close().await;
    Ok(ProcessMemoriesResult {
        processed_count,
        failed_count,
        missing_count,
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

#[tauri::command]
fn get_media_storage_path(app: tauri::AppHandle) -> Result<String, String> {
    resolve_output_dir(&app, ".raw_cache").map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn open_media_folder(app: tauri::AppHandle) -> Result<(), String> {
    let path = resolve_output_dir(&app, ".raw_cache")?;

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|error| format!("failed to open folder: {error}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("failed to open folder: {error}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("failed to open folder: {error}"))?;
    }

    Ok(())
}

#[tauri::command]
fn get_export_path(app: tauri::AppHandle) -> Result<String, String> {
    let base = resolve_base_dir(&app)?;
    Ok(base.to_string_lossy().to_string())
}

#[tauri::command]
fn get_default_export_path(app: tauri::AppHandle) -> Result<String, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|error| format!("failed to resolve app data directory: {error}"))
}

#[tauri::command]
fn set_export_path(app: tauri::AppHandle, path: Option<String>) -> Result<String, String> {
    if let Some(ref custom_path) = path {
        let target = std::path::Path::new(custom_path);
        if !target.is_absolute() {
            return Err("export path must be absolute".to_string());
        }
        std::fs::create_dir_all(target)
            .map_err(|error| format!("cannot create export directory: {error}"))?;

        // Verify writable by creating and immediately removing a probe file
        let probe = target.join(".memorysnaper_probe");
        std::fs::write(&probe, b"probe")
            .map_err(|_| "export directory is not writable".to_string())?;
        let _ = std::fs::remove_file(&probe);
    }

    {
        let config_state = app.state::<AppConfigState>();
        let mut config = config_state
            .0
            .lock()
            .map_err(|error| format!("config lock poisoned: {error}"))?;
        config.export_path = path;
        save_app_config(&app, &config)?;
    }

    // Allow the asset protocol to serve files from the new path
    let scope = app.asset_protocol_scope();
    let new_base = resolve_base_dir(&app)?;
    let _ = scope.allow_directory(&new_base, true);

    let resolved = resolve_base_dir(&app)?;
    Ok(resolved.to_string_lossy().to_string())
}

#[tauri::command]
fn get_disk_space(path: String) -> Result<DiskSpaceInfo, String> {
    let target = std::path::Path::new(&path);
    let lookup_path = if target.exists() {
        target.to_path_buf()
    } else if let Some(parent) = target.parent() {
        if parent.exists() {
            parent.to_path_buf()
        } else {
            target.to_path_buf()
        }
    } else {
        target.to_path_buf()
    };

    let total_bytes = fs2::total_space(&lookup_path)
        .map_err(|error| format!("failed to read total disk space: {error}"))?
        as u64;
    let free_bytes = fs2::available_space(&lookup_path)
        .map_err(|error| format!("failed to read available disk space: {error}"))?
        as u64;

    Ok(DiskSpaceInfo {
        total_bytes,
        free_bytes,
    })
}

#[tauri::command]
fn get_files_total_size(paths: Vec<String>) -> Result<u64, String> {
    let mut total: u64 = 0;
    for p in &paths {
        let meta = std::fs::metadata(p).map_err(|error| format!("cannot stat {p}: {error}"))?;
        total += meta.len();
    }
    Ok(total)
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

fn find_output_file_for_memory_item_recursive(
    output_dir: &std::path::Path,
    memory_item_id: i64,
) -> Result<Option<std::path::PathBuf>, std::io::Error> {
    if !output_dir.exists() {
        return Ok(None);
    }

    let target_stem = memory_item_id.to_string();
    let mut dir_stack = vec![output_dir.to_path_buf()];

    while let Some(current_dir) = dir_stack.pop() {
        for entry in std::fs::read_dir(&current_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let should_skip_dir = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == ".thumbnails" || name == ".staging");

                if !should_skip_dir {
                    dir_stack.push(path);
                }

                continue;
            }

            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };

            if stem != target_stem {
                continue;
            }

            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn viewer_media_kind_from_path(path: &std::path::Path) -> Option<ViewerMediaKind> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();

    if matches!(extension.as_str(), "mp4" | "mov" | "m4v" | "webm") {
        return Some(ViewerMediaKind::Video);
    }

    if matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp") {
        return Some(ViewerMediaKind::Image);
    }

    None
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

fn path_to_relative_string(
    path: &std::path::Path,
    root: &std::path::Path,
) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|error| {
        format!(
            "path '{}' is not under root '{}': {error}",
            path.display(),
            root.display()
        )
    })?;

    Ok(relative
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/"))
}

fn media_file_extension_allowed(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "mp4" | "mov" | "m4v" | "webm"
            )
        })
        .unwrap_or(false)
}

fn collect_media_files_recursive(
    root: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];

    while let Some(current) = directories.pop() {
        for entry in std::fs::read_dir(&current)
            .map_err(|error| format!("failed to read directory '{}': {error}", current.display()))?
        {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to read directory entry in '{}': {error}",
                    current.display()
                )
            })?;

            let path = entry.path();
            if path.is_dir() {
                let should_skip = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == ".thumbnails" || name == ".staging");

                if !should_skip {
                    directories.push(path);
                }

                continue;
            }

            if media_file_extension_allowed(&path) {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn collect_files_recursive(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];

    while let Some(current) = directories.pop() {
        for entry in std::fs::read_dir(&current)
            .map_err(|error| format!("failed to read directory '{}': {error}", current.display()))?
        {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to read directory entry in '{}': {error}",
                    current.display()
                )
            })?;

            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }

            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

fn zip_entry_name(relative_path: &std::path::Path) -> String {
    relative_path
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn write_file_to_zip(
    writer: &mut zip::ZipWriter<std::fs::File>,
    source_path: &std::path::Path,
    entry_name: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let mut source_file = std::fs::File::open(source_path).map_err(|error| {
        format!(
            "failed to open '{}' for zip write: {error}",
            source_path.display()
        )
    })?;

    writer
        .start_file(entry_name, options)
        .map_err(|error| format!("failed to start zip entry '{entry_name}': {error}"))?;

    std::io::copy(&mut source_file, writer).map_err(|error| {
        format!(
            "failed to copy '{}' into zip: {error}",
            source_path.display()
        )
    })?;

    Ok(())
}

fn extract_zip_archive(
    zip_path: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    let archive_file = std::fs::File::open(zip_path)
        .map_err(|error| format!("failed to open archive '{}': {error}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|error| format!("failed to read archive '{}': {error}", zip_path.display()))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("failed to read zip entry at index {index}: {error}"))?;

        let Some(safe_name) = entry.enclosed_name().map(|value| value.to_path_buf()) else {
            continue;
        };

        let output_path = destination.join(safe_name);
        if entry.is_dir() {
            std::fs::create_dir_all(&output_path).map_err(|error| {
                format!(
                    "failed to create extracted directory '{}': {error}",
                    output_path.display()
                )
            })?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create extracted parent directory '{}': {error}",
                    parent.display()
                )
            })?;
        }

        let mut output_file = std::fs::File::create(&output_path).map_err(|error| {
            format!(
                "failed to create extracted file '{}': {error}",
                output_path.display()
            )
        })?;

        std::io::copy(&mut entry, &mut output_file).map_err(|error| {
            format!(
                "failed to extract zip entry to '{}': {error}",
                output_path.display()
            )
        })?;
    }

    Ok(())
}

fn compute_blake3_hash(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("failed to open '{}' for hashing: {error}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 16 * 1024];

    loop {
        let read_count = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|error| format!("failed to read '{}' for hashing: {error}", path.display()))?;

        if read_count == 0 {
            break;
        }

        hasher.update(&buffer[..read_count]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[tauri::command]
async fn create_settings_media_backup_zip(
    app: tauri::AppHandle,
    archive_path: String,
) -> Result<ArchiveCreationResult, String> {
    let output_dir = resolve_output_dir(&app, ".raw_cache")?;
    let media_files = collect_media_files_recursive(&output_dir)?;
    let archive_path = std::path::PathBuf::from(&archive_path);

    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create archive parent directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let archive_file = std::fs::File::create(&archive_path).map_err(|error| {
        format!(
            "failed to create backup archive '{}': {error}",
            archive_path.display()
        )
    })?;
    let mut writer = zip::ZipWriter::new(archive_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));

    for media_file in &media_files {
        let relative_path = media_file.strip_prefix(&output_dir).map_err(|error| {
            format!(
                "failed to build relative backup path for '{}': {error}",
                media_file.display()
            )
        })?;

        let entry_name = format!("media/{}", zip_entry_name(relative_path));
        write_file_to_zip(&mut writer, media_file, &entry_name, options)?;
    }

    writer.finish().map_err(|error| {
        format!(
            "failed to finish backup archive '{}': {error}",
            archive_path.display()
        )
    })?;

    Ok(ArchiveCreationResult {
        archive_path: archive_path.to_string_lossy().to_string(),
        added_files: media_files.len(),
    })
}

#[tauri::command]
async fn create_viewer_export_zip(
    app: tauri::AppHandle,
    archive_path: String,
) -> Result<ArchiveCreationResult, String> {
    let output_dir = resolve_output_dir(&app, ".raw_cache")?;
    let media_files = collect_media_files_recursive(&output_dir)?;
    let thumbnails_dir = output_dir.join(".thumbnails");
    let thumbnail_files = collect_files_recursive(&thumbnails_dir)?;

    let mut db_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    db_path.push("memories.db");

    if !db_path.exists() {
        return Err(format!(
            "viewer export database file not found at '{}'",
            db_path.display()
        ));
    }

    let archive_path = std::path::PathBuf::from(&archive_path);
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create archive parent directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let archive_file = std::fs::File::create(&archive_path).map_err(|error| {
        format!(
            "failed to create viewer export archive '{}': {error}",
            archive_path.display()
        )
    })?;
    let mut writer = zip::ZipWriter::new(archive_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));

    let manifest = ViewerArchiveManifest {
        archive_type: "viewer-export".to_string(),
        version: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let manifest_payload = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("failed to serialize viewer archive manifest: {error}"))?;

    writer
        .start_file(VIEWER_ARCHIVE_MANIFEST_NAME, options)
        .map_err(|error| format!("failed to create viewer manifest entry: {error}"))?;
    std::io::Write::write_all(&mut writer, &manifest_payload)
        .map_err(|error| format!("failed to write viewer manifest entry: {error}"))?;

    write_file_to_zip(&mut writer, &db_path, "db/memories.db", options)?;

    for thumbnail_file in &thumbnail_files {
        let relative_path = thumbnail_file
            .strip_prefix(&thumbnails_dir)
            .map_err(|error| {
                format!(
                    "failed to build relative thumbnail path for '{}': {error}",
                    thumbnail_file.display()
                )
            })?;
        let entry_name = format!("thumbnails/{}", zip_entry_name(relative_path));
        write_file_to_zip(&mut writer, thumbnail_file, &entry_name, options)?;
    }

    for media_file in &media_files {
        let relative_path = media_file.strip_prefix(&output_dir).map_err(|error| {
            format!(
                "failed to build relative media path for '{}': {error}",
                media_file.display()
            )
        })?;
        let entry_name = format!("media/{}", zip_entry_name(relative_path));
        write_file_to_zip(&mut writer, media_file, &entry_name, options)?;
    }

    writer.finish().map_err(|error| {
        format!(
            "failed to finish viewer archive '{}': {error}",
            archive_path.display()
        )
    })?;

    Ok(ArchiveCreationResult {
        archive_path: archive_path.to_string_lossy().to_string(),
        added_files: media_files.len() + thumbnail_files.len() + 2,
    })
}

#[tauri::command]
async fn import_viewer_export_zip(
    app: tauri::AppHandle,
    archive_path: String,
) -> Result<ViewerArchiveImportResult, String> {
    let archive_path = std::path::PathBuf::from(&archive_path);
    if !archive_path.exists() {
        return Err(format!(
            "archive '{}' does not exist",
            archive_path.display()
        ));
    }

    let import_root = std::env::temp_dir().join(format!(
        "memorysnaper-viewer-import-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::create_dir_all(&import_root).map_err(|error| {
        format!(
            "failed to create temporary import directory '{}': {error}",
            import_root.display()
        )
    })?;

    let result = async {
        extract_zip_archive(&archive_path, &import_root)?;

        let manifest_path = import_root.join(VIEWER_ARCHIVE_MANIFEST_NAME);
        let manifest_payload = std::fs::read_to_string(&manifest_path).map_err(|error| {
            format!(
                "failed to read viewer archive manifest '{}': {error}",
                manifest_path.display()
            )
        })?;
        let manifest: ViewerArchiveManifest = serde_json::from_str(&manifest_payload)
            .map_err(|error| format!("failed to parse viewer archive manifest: {error}"))?;

        if manifest.archive_type != "viewer-export" {
            return Err(format!(
                "unsupported archive type '{}'; expected 'viewer-export'",
                manifest.archive_type
            ));
        }

        let source_db_path = import_root.join("db").join("memories.db");
        if !source_db_path.exists() {
            return Err(format!(
                "viewer import archive is missing database file at '{}'",
                source_db_path.display()
            ));
        }

        let source_media_dir = import_root.join("media");
        let source_thumbnail_dir = import_root.join("thumbnails");

        let source_db_url = core::sqlite_url_from_path(&source_db_path);
        let target_db_url = memories_db_url(&app)?;

        let source_pool = sqlx::SqlitePool::connect(&source_db_url)
            .await
            .map_err(|error| format!("failed to connect to source viewer database: {error}"))?;
        let target_pool = sqlx::SqlitePool::connect(&target_db_url)
            .await
            .map_err(|error| format!("failed to connect to target viewer database: {error}"))?;

        let existing_hash_rows = sqlx::query(
            "SELECT content_hash FROM Memories WHERE content_hash IS NOT NULL AND TRIM(content_hash) != ''",
        )
        .fetch_all(&target_pool)
        .await
        .map_err(|error| format!("failed to load existing memory hashes: {error}"))?;

        let mut known_hashes = existing_hash_rows
            .into_iter()
            .filter_map(|row| row.try_get::<String, _>("content_hash").ok())
            .collect::<std::collections::HashSet<_>>();

        let source_rows = sqlx::query(
            "
            SELECT
                mi.id AS source_memory_item_id,
                COALESCE(mi.date_time, mi.date) AS date_taken,
                mi.location AS location_raw,
                mi.location_resolved AS location_resolved,
                m.mid AS source_mid,
                m.content_hash AS source_content_hash
            FROM MemoryItem mi
            LEFT JOIN MediaChunks mc
              ON mc.url = mi.media_url
             AND IFNULL(mc.overlay_url, '') = IFNULL(mi.overlay_url, '')
             AND mc.order_index = 1
            LEFT JOIN Memories m
              ON m.id = mc.memory_id
            WHERE mi.status = 'processed'
            ORDER BY mi.id ASC
            ",
        )
        .fetch_all(&source_pool)
        .await
        .map_err(|error| format!("failed to load source viewer records: {error}"))?;

        let output_dir = resolve_output_dir(&app, ".raw_cache")?;
        let thumbnail_dir = output_dir.join(".thumbnails");
        std::fs::create_dir_all(&output_dir)
            .map_err(|error| format!("failed to create output directory '{}': {error}", output_dir.display()))?;
        std::fs::create_dir_all(&thumbnail_dir).map_err(|error| {
            format!(
                "failed to create thumbnail directory '{}': {error}",
                thumbnail_dir.display()
            )
        })?;

        let mut imported_count = 0usize;
        let mut skipped_count = 0usize;

        for row in source_rows {
            let source_memory_item_id = row.get::<i64, _>("source_memory_item_id");
            let date_taken = row.get::<String, _>("date_taken");
            let location_raw = row.try_get::<Option<String>, _>("location_raw").ok().flatten();
            let location_resolved = row
                .try_get::<Option<String>, _>("location_resolved")
                .ok()
                .flatten();
            let source_mid = row.try_get::<Option<String>, _>("source_mid").ok().flatten();

            let Some(source_media_path) = find_output_file_for_memory_item_recursive(
                &source_media_dir,
                source_memory_item_id,
            )
            .map_err(|error| {
                format!(
                    "failed to locate source media for item {}: {error}",
                    source_memory_item_id
                )
            })?
            else {
                skipped_count += 1;
                continue;
            };

            let source_thumbnail_path = source_thumbnail_dir.join(format!("{source_memory_item_id}.webp"));
            if !source_thumbnail_path.exists() {
                skipped_count += 1;
                continue;
            }

            let content_hash = match row.try_get::<Option<String>, _>("source_content_hash") {
                Ok(Some(hash)) if !hash.trim().is_empty() => hash,
                _ => compute_blake3_hash(&source_media_path)?,
            };

            if known_hashes.contains(&content_hash) {
                skipped_count += 1;
                continue;
            }

            let media_extension = source_media_path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_else(|| "bin".to_string());

            let insert_memory_item_result = sqlx::query(
                "
                INSERT INTO MemoryItem (
                    date,
                    location,
                    media_url,
                    overlay_url,
                    status,
                    retry_count,
                    last_error_code,
                    last_error_message,
                    date_time,
                    location_resolved
                )
                VALUES (?1, ?2, ?3, NULL, 'processed', 0, NULL, NULL, ?4, ?5)
                ",
            )
            .bind(&date_taken)
            .bind(&location_raw)
            .bind(format!("viewer-import://{source_memory_item_id}"))
            .bind(&date_taken)
            .bind(&location_resolved)
            .execute(&target_pool)
            .await
            .map_err(|error| format!("failed to insert imported MemoryItem row: {error}"))?;

            let new_memory_item_id = insert_memory_item_result.last_insert_rowid();
            let target_media_path = output_dir.join(format!("{new_memory_item_id}.{media_extension}"));
            let target_thumbnail_path = thumbnail_dir.join(format!("{new_memory_item_id}.webp"));

            std::fs::copy(&source_media_path, &target_media_path).map_err(|error| {
                format!(
                    "failed to copy imported media '{}' to '{}': {error}",
                    source_media_path.display(),
                    target_media_path.display()
                )
            })?;
            std::fs::copy(&source_thumbnail_path, &target_thumbnail_path).map_err(|error| {
                format!(
                    "failed to copy imported thumbnail '{}' to '{}': {error}",
                    source_thumbnail_path.display(),
                    target_thumbnail_path.display()
                )
            })?;

            let relative_media_path = target_media_path
                .strip_prefix(&output_dir)
                .map_err(|error| {
                    format!(
                        "failed to compute imported media relative path for '{}': {error}",
                        target_media_path.display()
                    )
                })?
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            let relative_thumbnail_path = format!(".thumbnails/{new_memory_item_id}.webp");
            let memory_hash = format!("import:{content_hash}");

            let insert_memories_result = sqlx::query(
                "
                INSERT INTO Memories (
                    hash,
                    date,
                    status,
                    job_id,
                    mid,
                    content_hash,
                    relative_path,
                    thumbnail_path
                )
                VALUES (?1, ?2, 'PROCESSED', NULL, ?3, ?4, ?5, ?6)
                ",
            )
            .bind(&memory_hash)
            .bind(&date_taken)
            .bind(&source_mid)
            .bind(&content_hash)
            .bind(&relative_media_path)
            .bind(&relative_thumbnail_path)
            .execute(&target_pool)
            .await
            .map_err(|error| format!("failed to insert imported Memories row: {error}"))?;

            let new_memory_group_id = insert_memories_result.last_insert_rowid();

            sqlx::query(
                "
                INSERT INTO MediaChunks (memory_id, url, overlay_url, order_index)
                VALUES (?1, ?2, NULL, 1)
                ",
            )
            .bind(new_memory_group_id)
            .bind(format!("viewer-import://{new_memory_item_id}"))
            .execute(&target_pool)
            .await
            .map_err(|error| format!("failed to insert imported MediaChunks row: {error}"))?;

            known_hashes.insert(content_hash);
            imported_count += 1;
        }

        source_pool.close().await;
        target_pool.close().await;

        Ok(ViewerArchiveImportResult {
            imported_count,
            skipped_count,
        })
    }
    .await;

    if let Err(error) = std::fs::remove_dir_all(&import_root) {
        eprintln!(
            "[viewer-import] failed to remove temp import directory '{}': {}",
            import_root.display(),
            error
        );
    }

    result
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

async fn backfill_metadata(database_url: String) {
    let pool = match sqlx::SqlitePool::connect(&database_url).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("[backfill] failed to connect to database: {error}");
            return;
        }
    };

    // Warm up the geocoder in a blocking thread so the KD-tree build
    // doesn't stall the async runtime on first call.
    tokio::task::spawn_blocking(|| {
        let _ = crate::core::geocoder::resolve_location("0,0");
    })
    .await
    .ok();

    let rows = match sqlx::query(
        "SELECT id, date, location FROM MemoryItem
         WHERE date_time IS NULL OR location_resolved IS NULL",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("[backfill] failed to query MemoryItem: {error}");
            return;
        }
    };

    if rows.is_empty() {
        eprintln!("[backfill] no rows to backfill");
        pool.close().await;
        return;
    }

    // Process enrichment: collect all updates to batch them
    #[derive(Debug)]
    struct Enriched {
        id: i64,
        date_time: Option<String>,
        location_resolved: Option<String>,
    }

    let mut enriched_rows = Vec::with_capacity(rows.len());
    let mut errors = Vec::new();

    for row in rows {
        let id: i64 = row.get("id");
        let date: String = row.get("date");
        let location: Option<String> = row.try_get("location").ok().flatten();

        let date_time = crate::core::parser::extract_full_datetime(&date);
        let location_resolved = location
            .as_deref()
            .and_then(crate::core::geocoder::resolve_location);

        enriched_rows.push(Enriched {
            id,
            date_time,
            location_resolved,
        });
    }

    eprintln!(
        "[backfill] enriched {} rows, processing batch updates",
        enriched_rows.len()
    );

    // Batch updates in groups of 100 to avoid query size limits
    const BATCH_SIZE: usize = 100;
    for chunk in enriched_rows.chunks(BATCH_SIZE) {
        let mut query_builder =
            sqlx::query_builder::QueryBuilder::new("UPDATE MemoryItem SET date_time = CASE id ");

        for enriched in chunk {
            query_builder.push("WHEN ");
            query_builder.push_bind(enriched.id);
            query_builder.push(" THEN ");
            query_builder.push_bind(&enriched.date_time);
        }

        query_builder.push(" END, location_resolved = CASE id ");

        for enriched in chunk {
            query_builder.push("WHEN ");
            query_builder.push_bind(enriched.id);
            query_builder.push(" THEN ");
            query_builder.push_bind(&enriched.location_resolved);
        }

        query_builder.push(" END WHERE id IN (");
        let mut first = true;
        for enriched in chunk {
            if !first {
                query_builder.push(", ");
            }
            query_builder.push_bind(enriched.id);
            first = false;
        }
        query_builder.push(")");

        let query = query_builder.build();

        if let Err(error) = query.execute(&pool).await {
            eprintln!(
                "[backfill] failed to batch update {} rows: {error}",
                chunk.len()
            );
            errors.push(error.to_string());
        }
    }

    if !errors.is_empty() {
        eprintln!(
            "[backfill] completed with {} errors out of {} batches",
            errors.len(),
            enriched_rows.len().div_ceil(BATCH_SIZE)
        );
    } else {
        eprintln!(
            "[backfill] successfully enriched {} rows",
            enriched_rows.len()
        );
    }

    pool.close().await;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Load persisted config (export path) before anything else
            let config = load_app_config(app.handle());
            app.manage(AppConfigState(Mutex::new(config.clone())));

            // If a custom export path is configured, allow asset protocol access
            if config.export_path.is_some() {
                if let Ok(base) = resolve_base_dir(app.handle()) {
                    let scope = app.asset_protocol_scope();
                    let _ = scope.allow_directory(&base, true);
                }
            }

            tauri::async_runtime::block_on(setup_database(app.handle()))
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            let database_url = memories_db_url(app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            tauri::async_runtime::spawn(backfill_metadata(database_url));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_system_locale,
            probe_system_codecs,
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
            get_missing_files,
            get_missing_file_by_memory_item_id,
            download_queued_memories,
            resume_export_downloads,
            apply_metadata_to_output_files,
            process_memories_from_zip_archives,
            process_downloaded_memories,
            get_thumbnails,
            get_viewer_items,
            has_viewer_items,
            get_media_storage_path,
            open_media_folder,
            create_settings_media_backup_zip,
            create_viewer_export_zip,
            import_viewer_export_zip,
            reset_all_app_data,
            get_export_path,
            get_default_export_path,
            set_export_path,
            get_disk_space,
            get_files_total_size
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

        let statuses: Vec<String> =
            sqlx::query_scalar("SELECT status FROM MemoryItem WHERE id IN (5, 6) ORDER BY id ASC")
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

    #[tokio::test]
    async fn pause_flag_pauses_processing_loop_without_crashing() {
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

        sqlx::query("INSERT INTO MemoryItem (id, status) VALUES (10, 'queued')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO Memories (id, status) VALUES (300, 'queued')")
            .execute(&pool)
            .await
            .unwrap();

        let units = vec![ProcessUnit {
            memory_group_id: Some(300),
            progress_item_id: 10,
            memory_item_ids: vec![10],
            date_taken: "2026-02-20".to_string(),
            location: None,
        }];

        // Spawn a task that will pause after a short delay
        let pause_handle = {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                crate::core::state::set_paused(true);
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                crate::core::state::set_paused(false);
            })
        };

        // This should pause while is_paused is true, then return when is_paused is set to false
        let result = wait_for_pause_or_stop_and_mark_pending(&pool, &units).await;

        pause_handle.await.expect("pause task should complete");

        // Should return Ok(false) because we never set stopped
        assert!(result.is_ok(), "should not error");
        assert_eq!(
            result.unwrap(),
            false,
            "should return false when paused then resumed (not stopped)"
        );

        // Item status should still be 'queued' since we didn't stop
        let item_status: String = sqlx::query_scalar("SELECT status FROM MemoryItem WHERE id = 10")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            item_status, "queued",
            "item should still be queued when resumed"
        );

        crate::core::state::reset();
    }
}
