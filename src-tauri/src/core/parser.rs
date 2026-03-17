use std::fmt::{Display, Formatter};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug)]
pub enum ParserError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    InvalidInputExtension(String),
    MissingMemoriesHistoryJson,
    Join(tokio::task::JoinError),
    InvalidSchema(String),
    Json(serde_json::Error),
    Database(sqlx::Error),
    NoImportableRecords,
}

impl Display for ParserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "failed to read memories history file: {error}"),
            Self::Zip(error) => write!(f, "failed to extract zip archive: {error}"),
            Self::InvalidInputExtension(path) => {
                write!(
                    f,
                    "unsupported input file extension for memories import: {path}"
                )
            }
            Self::MissingMemoriesHistoryJson => {
                write!(f, "zip archive does not contain memories_history.json")
            }
            Self::Join(error) => write!(f, "failed to run zip extraction task: {error}"),
            Self::InvalidSchema(reason) => {
                write!(f, "memories history json does not match expected schema: {reason}")
            }
            Self::Json(error) => write!(f, "failed to parse memories history json: {error}"),
            Self::Database(error) => write!(f, "failed to persist memories into sqlite: {error}"),
            Self::NoImportableRecords => {
                write!(
                    f,
                    "no importable records were found in memories_history.json"
                )
            }
        }
    }
}

impl std::error::Error for ParserError {}

impl From<std::io::Error> for ParserError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ParserError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<zip::result::ZipError> for ParserError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}

impl From<tokio::task::JoinError> for ParserError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value)
    }
}

impl From<sqlx::Error> for ParserError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedMemoryItem {
    date: String,
    location: Option<String>,
    media_type: String,
    media_url: String,
    overlay_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub parsed_count: usize,
    pub imported_count: usize,
    pub skipped_duplicates: usize,
}

pub async fn import_memories_history_file(
    database_url: &str,
    memories_history_path: &Path,
) -> Result<usize, ParserError> {
    let json_content = load_memories_history_json(memories_history_path).await?;
    let summary = import_memories_history_json(database_url, &json_content).await?;

    Ok(summary.imported_count)
}

pub async fn validate_memories_history_file(memories_history_path: &Path) -> Result<(), ParserError> {
    let json_content = load_memories_history_json(memories_history_path).await?;
    let json_value: Value = serde_json::from_str(&json_content)?;

    validate_snapchat_export_schema(&json_value)
}

pub async fn load_memories_history_json(input_path: &Path) -> Result<String, ParserError> {
    let extension = input_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| ParserError::InvalidInputExtension(input_path.display().to_string()))?;

    match extension.as_str() {
        "json" => Ok(tokio::fs::read_to_string(input_path).await?),
        "zip" => {
            let extracted_path = extract_memories_history_json_to_temp(input_path).await?;
            Ok(tokio::fs::read_to_string(extracted_path).await?)
        }
        _ => Err(ParserError::InvalidInputExtension(
            input_path.display().to_string(),
        )),
    }
}

async fn extract_memories_history_json_to_temp(zip_path: &Path) -> Result<std::path::PathBuf, ParserError> {
    let zip_path = zip_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut file_index = None;

        for index in 0..archive.len() {
            let file = archive.by_index(index)?;
            let file_name = Path::new(file.name())
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();

            if file_name == "memories_history.json" {
                file_index = Some(index);
                break;
            }
        }

        let selected_index = file_index.ok_or(ParserError::MissingMemoriesHistoryJson)?;
        let mut selected_file = archive.by_index(selected_index)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();

        let temp_dir = std::env::temp_dir().join(format!("memorysnaper-{timestamp}"));
        std::fs::create_dir_all(&temp_dir)?;

        let output_path = temp_dir.join("memories_history.json");
        let mut output_file = std::fs::File::create(&output_path)?;
        std::io::copy(&mut selected_file, &mut output_file)?;
        output_file.flush()?;

        Ok::<std::path::PathBuf, ParserError>(output_path)
    })
    .await?
}

pub async fn import_memories_history_json(
    database_url: &str,
    json_content: &str,
) -> Result<ImportSummary, ParserError> {
    let json_value: Value = serde_json::from_str(json_content)?;
    validate_snapchat_export_schema(&json_value)?;

    let mut parsed_items = Vec::new();
    collect_memory_items(&json_value, &mut parsed_items);

    if parsed_items.is_empty() {
        return Err(ParserError::NoImportableRecords);
    }

    let pool = SqlitePool::connect(database_url).await?;
    let mut transaction = pool.begin().await?;
    let mut imported_count = 0usize;
    let mut skipped_duplicates = 0usize;
    let mut next_chunk_order_by_hash = std::collections::HashMap::<String, i64>::new();

    for item in &parsed_items {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM MemoryItem WHERE media_url = ?1)",
        )
        .bind(&item.media_url)
        .fetch_one(&mut *transaction)
        .await?;

        if exists == 1 {
            skipped_duplicates += 1;
            continue;
        }

        sqlx::query(
            "
            INSERT INTO MemoryItem (date, location, media_url, overlay_url, status)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
        )
        .bind(&item.date)
        .bind(&item.location)
        .bind(&item.media_url)
        .bind(&item.overlay_url)
        .bind("queued")
        .execute(&mut *transaction)
        .await?;

        imported_count += 1;

        let memory_hash = build_memory_hash(&item.date, &item.media_type);

        sqlx::query(
            "
            INSERT OR IGNORE INTO Memories (hash, date, status)
            VALUES (?1, ?2, ?3)
            ",
        )
        .bind(&memory_hash)
        .bind(&item.date)
        .bind("queued")
        .execute(&mut *transaction)
        .await?;

        let memory_id = sqlx::query_scalar::<_, i64>("SELECT id FROM Memories WHERE hash = ?1")
            .bind(&memory_hash)
            .fetch_one(&mut *transaction)
            .await?;

        let chunk_exists = sqlx::query_scalar::<_, i64>(
            "
            SELECT EXISTS(
                SELECT 1 FROM MediaChunks
                WHERE memory_id = ?1
                  AND url = ?2
                  AND IFNULL(overlay_url, '') = IFNULL(?3, '')
            )
            ",
        )
        .bind(memory_id)
        .bind(&item.media_url)
        .bind(&item.overlay_url)
        .fetch_one(&mut *transaction)
        .await?;

        if chunk_exists == 0 {
            let order_index = if let Some(next_order_index) = next_chunk_order_by_hash.get_mut(&memory_hash) {
                let current_order_index = *next_order_index;
                *next_order_index += 1;
                current_order_index
            } else {
                let next_order_index = sqlx::query_scalar::<_, Option<i64>>(
                    "SELECT COALESCE(MAX(order_index), 0) FROM MediaChunks WHERE memory_id = ?1",
                )
                .bind(memory_id)
                .fetch_one(&mut *transaction)
                .await?
                .unwrap_or(0)
                    + 1;

                next_chunk_order_by_hash.insert(memory_hash.clone(), next_order_index + 1);
                next_order_index
            };

            sqlx::query(
                "
                INSERT INTO MediaChunks (memory_id, url, overlay_url, order_index)
                VALUES (?1, ?2, ?3, ?4)
                ",
            )
            .bind(memory_id)
            .bind(&item.media_url)
            .bind(&item.overlay_url)
            .bind(order_index)
            .execute(&mut *transaction)
            .await?;
        }
    }

    transaction.commit().await?;
    pool.close().await;

    Ok(ImportSummary {
        parsed_count: parsed_items.len(),
        imported_count,
        skipped_duplicates,
    })
}

fn validate_snapchat_export_schema(value: &Value) -> Result<(), ParserError> {
    let Value::Object(_) = value else {
        return Err(ParserError::InvalidSchema(
            "top-level JSON value must be an object".to_string(),
        ));
    };

    let mut has_supported_container = false;
    let mut has_importable_item = false;
    scan_schema_signals(
        value,
        &mut has_supported_container,
        &mut has_importable_item,
    );

    if !has_supported_container {
        return Err(ParserError::InvalidSchema(
            "missing expected Snapchat memories container (for example `Saved Media` or `memories`)"
                .to_string(),
        ));
    }

    if !has_importable_item {
        return Err(ParserError::InvalidSchema(
            "no importable memory items with supported media URL fields were found".to_string(),
        ));
    }

    Ok(())
}

fn scan_schema_signals(
    value: &Value,
    has_supported_container: &mut bool,
    has_importable_item: &mut bool,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                scan_schema_signals(item, has_supported_container, has_importable_item);
            }
        }
        Value::Object(map) => {
            if parse_memory_item(value).is_some() {
                *has_importable_item = true;
            }

            for (key, nested_value) in map {
                if is_supported_snapchat_container_key(key)
                    && matches!(nested_value, Value::Array(_) | Value::Object(_))
                {
                    *has_supported_container = true;
                }

                scan_schema_signals(nested_value, has_supported_container, has_importable_item);
            }
        }
        _ => {}
    }
}

fn is_supported_snapchat_container_key(key: &str) -> bool {
    matches!(
        key,
        "Saved Media"
            | "Saved Stories"
            | "Memories"
            | "memories"
            | "savedMedia"
            | "saved_memories"
    )
}

fn collect_memory_items(value: &Value, parsed_items: &mut Vec<ParsedMemoryItem>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_memory_items(item, parsed_items);
            }
        }
        Value::Object(map) => {
            if let Some(parsed_item) = parse_memory_item(value) {
                parsed_items.push(parsed_item);
            }

            for nested_value in map.values() {
                collect_memory_items(nested_value, parsed_items);
            }
        }
        _ => {}
    }
}

fn parse_memory_item(value: &Value) -> Option<ParsedMemoryItem> {
    let Value::Object(_) = value else {
        return None;
    };

    let media_url = first_non_empty_string(
        value,
        &[
            "media_url",
            "mediaUrl",
            "download_link",
            "downloadLink",
            "download_url",
            "downloadUrl",
            "Media URL",
            "Download Link",
        ],
    )?;

    let overlay_url = first_non_empty_string(
        value,
        &[
            "overlay_url",
            "overlayUrl",
            "overlay_download_link",
            "overlayDownloadLink",
            "overlay_download_url",
            "overlayDownloadUrl",
            "Overlay URL",
            "Overlay Download Link",
        ],
    );

    let date = first_non_empty_string(
        value,
        &[
            "date",
            "created_at",
            "createdAt",
            "Date",
            "Creation Timestamp",
            "Saved Timestamp",
        ],
    )
    .unwrap_or_else(|| "unknown".to_string());

    let location = parse_location(value);
    let media_type = infer_media_type(&media_url).to_string();

    Some(ParsedMemoryItem {
        date,
        location,
        media_type,
        media_url,
        overlay_url,
    })
}

fn infer_media_type(media_url: &str) -> &'static str {
    let extension = media_url
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .and_then(|extension| extension.split(['?', '#']).next())
        .map(|extension| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("mp4") | Some("mov") | Some("m4v") | Some("avi") | Some("webm") => "video",
        Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("webp") | Some("heic") => {
            "image"
        }
        _ => "unknown",
    }
}

fn build_memory_hash(date: &str, media_type: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(date.as_bytes());
    hasher.update(b"|");
    hasher.update(media_type.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn first_non_empty_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = value.get(key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

fn parse_location(value: &Value) -> Option<String> {
    if let Some(location) = first_non_empty_string(value, &["location", "Location"]) {
        return Some(location);
    }

    let latitude = first_non_empty_string(value, &["latitude", "Latitude"]);
    let longitude = first_non_empty_string(value, &["longitude", "Longitude"]);

    match (latitude, longitude) {
        (Some(latitude), Some(longitude)) => Some(format!("{latitude},{longitude}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_memory_items, import_memories_history_file, import_memories_history_json,
        validate_snapchat_export_schema, ParserError,
    };
    use sqlx::Row;

    #[test]
    fn collects_nested_memory_items() {
        let input = serde_json::json!({
            "memories": [
                {
                    "media_url": "https://example.com/media-1.jpg",
                    "date": "2024-01-01T10:00:00Z",
                    "location": "Berlin"
                },
                {
                    "nested": {
                        "Download Link": "https://example.com/media-2.mp4",
                        "Saved Timestamp": "2024-01-02T10:00:00Z",
                        "overlayUrl": "https://example.com/overlay-2.png"
                    }
                }
            ]
        });

        let mut parsed_items = Vec::new();
        collect_memory_items(&input, &mut parsed_items);

        assert_eq!(parsed_items.len(), 2);
        assert_eq!(parsed_items[0].media_url, "https://example.com/media-1.jpg");
        assert_eq!(
            parsed_items[1].overlay_url.as_deref(),
            Some("https://example.com/overlay-2.png")
        );
    }

    #[test]
    fn validates_supported_snapchat_schema() {
        let input = serde_json::json!({
            "Saved Media": [
                {
                    "mediaUrl": "https://example.com/media-1.jpg",
                    "createdAt": "2024-03-01T00:00:00Z"
                }
            ]
        });

        let result = validate_snapchat_export_schema(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_unsupported_schema_shape() {
        let input = serde_json::json!({
            "items": [
                {
                    "url": "https://example.com/not-supported.jpg",
                    "date": "2024-03-01T00:00:00Z"
                }
            ]
        });

        let result = validate_snapchat_export_schema(&input);
        assert!(matches!(result, Err(ParserError::InvalidSchema(_))));
    }

    #[tokio::test]
    async fn imports_records_into_sqlite() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for parser tests");
        let db_path = temp_dir.path().join("memories.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

        let pool = sqlx::SqlitePool::connect(&db_url)
            .await
            .expect("sqlite pool should be created for parser tests");

        create_import_tables(&pool).await;

        pool.close().await;

        let json_path = temp_dir.path().join("memories_history.json");
        tokio::fs::write(
            &json_path,
            serde_json::json!({
                "Saved Media": [
                    {
                        "mediaUrl": "https://example.com/1.jpg",
                        "createdAt": "2024-03-01T00:00:00Z",
                        "location": "Paris"
                    },
                    {
                        "download_url": "https://example.com/2.mp4",
                        "overlay_download_url": "https://example.com/2-overlay.png",
                        "Date": "2024-03-02T00:00:00Z"
                    }
                ]
            })
            .to_string(),
        )
        .await
        .expect("json test fixture should be written");

        let inserted_count = import_memories_history_file(&db_url, &json_path)
            .await
            .expect("records should be imported into sqlite");

        assert_eq!(inserted_count, 2);

        let verification_pool = sqlx::SqlitePool::connect(&db_url)
            .await
            .expect("sqlite pool should be opened for verification");

        let count_row = sqlx::query("SELECT COUNT(*) AS count FROM MemoryItem")
            .fetch_one(&verification_pool)
            .await
            .expect("count query should execute");
        let count = count_row.get::<i64, _>("count");

        assert_eq!(count, 2);

        verification_pool.close().await;
    }

    #[tokio::test]
    async fn skips_duplicate_media_urls_during_import() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for parser tests");
        let db_path = temp_dir.path().join("memories.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

        let pool = sqlx::SqlitePool::connect(&db_url)
            .await
            .expect("sqlite pool should be created for parser tests");

        create_import_tables(&pool).await;

        pool.close().await;

        let summary = import_memories_history_json(
            &db_url,
            &serde_json::json!({
                "Saved Media": [
                    {
                        "mediaUrl": "https://example.com/duplicate.jpg",
                        "createdAt": "2024-03-01T00:00:00Z"
                    },
                    {
                        "mediaUrl": "https://example.com/duplicate.jpg",
                        "createdAt": "2024-03-02T00:00:00Z"
                    }
                ]
            })
            .to_string(),
        )
        .await
        .expect("import should succeed");

        assert_eq!(summary.parsed_count, 2);
        assert_eq!(summary.imported_count, 1);
        assert_eq!(summary.skipped_duplicates, 1);
    }

    #[tokio::test]
    async fn returns_no_importable_records_for_unsupported_shapes() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for parser tests");
        let db_path = temp_dir.path().join("memories.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

        let pool = sqlx::SqlitePool::connect(&db_url)
            .await
            .expect("sqlite pool should be created for parser tests");

        create_import_tables(&pool).await;

        pool.close().await;

        let result = import_memories_history_json(
            &db_url,
            &serde_json::json!({
                "items": [
                    {
                        "url": "https://example.com/not-supported.jpg",
                        "date": "2024-03-01T00:00:00Z"
                    }
                ]
            })
            .to_string(),
        )
        .await;

        assert!(matches!(result, Err(ParserError::InvalidSchema(_))));
    }

    #[tokio::test]
    async fn deduplicates_memories_and_keeps_multipart_chunks() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for parser tests");
        let db_path = temp_dir.path().join("memories.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

        let pool = sqlx::SqlitePool::connect(&db_url)
            .await
            .expect("sqlite pool should be created for parser tests");
        create_import_tables(&pool).await;
        pool.close().await;

        let json_input = serde_json::json!({
            "Saved Media": [
                {
                    "mediaUrl": "https://example.com/video-part-1.mp4",
                    "createdAt": "2024-03-01T00:00:00Z"
                },
                {
                    "mediaUrl": "https://example.com/video-part-2.mp4",
                    "createdAt": "2024-03-01T00:00:00Z"
                },
                {
                    "mediaUrl": "https://example.com/photo-1.jpg",
                    "createdAt": "2024-03-01T00:00:00Z"
                }
            ]
        })
        .to_string();

        let first_summary = import_memories_history_json(&db_url, &json_input)
            .await
            .expect("first import should succeed");
        let second_summary = import_memories_history_json(&db_url, &json_input)
            .await
            .expect("second import should succeed");

        assert_eq!(first_summary.imported_count, 3);
        assert_eq!(second_summary.imported_count, 0);

        let verification_pool = sqlx::SqlitePool::connect(&db_url)
            .await
            .expect("sqlite pool should be opened for verification");

        let memories_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM Memories")
            .fetch_one(&verification_pool)
            .await
            .expect("memories count query should execute");
        assert_eq!(memories_count, 2);

        let chunk_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM MediaChunks")
            .fetch_one(&verification_pool)
            .await
            .expect("media chunks count query should execute");
        assert_eq!(chunk_count, 3);

        let video_chunks = sqlx::query_scalar::<_, i64>(
            "
            SELECT COUNT(*)
            FROM MediaChunks mc
            INNER JOIN Memories m ON m.id = mc.memory_id
            WHERE mc.url LIKE '%video-part-%'
            ",
        )
        .fetch_one(&verification_pool)
        .await
        .expect("video chunk count query should execute");
        assert_eq!(video_chunks, 2);

        verification_pool.close().await;
    }

    async fn create_import_tables(pool: &sqlx::SqlitePool) {
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS MemoryItem (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                location TEXT,
                media_url TEXT NOT NULL,
                overlay_url TEXT,
                status TEXT NOT NULL
            )
            ",
        )
        .execute(pool)
        .await
        .expect("memory table should be created for parser tests");

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS Memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hash TEXT NOT NULL UNIQUE,
                date TEXT NOT NULL,
                status TEXT NOT NULL
            )
            ",
        )
        .execute(pool)
        .await
        .expect("memories table should be created for parser tests");

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS MediaChunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id INTEGER NOT NULL,
                url TEXT NOT NULL,
                overlay_url TEXT,
                order_index INTEGER NOT NULL,
                FOREIGN KEY (memory_id) REFERENCES Memories(id)
            )
            ",
        )
        .execute(pool)
        .await
        .expect("media chunks table should be created for parser tests");
    }
}
