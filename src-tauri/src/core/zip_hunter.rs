use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

/// Metadata for a single file entry discovered inside a ZIP archive.
///
/// This records enough information for later steps to match `main` / `overlay`
/// filenames and extract only the selected entries into `.staging/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipArchiveEntry {
    pub zip_path: PathBuf,
    pub entry_index: usize,
    pub entry_name: String,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}

/// Result of scanning the provided ZIP archives for a specific memory target.
///
/// Step 3.1 is limited to opening each ZIP and iterating through its entries
/// without extracting the full archive. Later steps will use `entries` to
/// identify and stage only the exact `main` / `overlay` files for this memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipMemoryScan {
    pub date: String,
    pub mid: String,
    pub entries: Vec<ZipArchiveEntry>,
    pub main_entry: Option<ZipArchiveEntry>,
    pub overlay_entry: Option<ZipArchiveEntry>,
    pub staging_dir: Option<PathBuf>,
    pub staged_main_path: Option<PathBuf>,
    pub staged_overlay_path: Option<PathBuf>,
    pub used_network_fallback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryEntryKind {
    Main,
    Overlay,
}

#[derive(Debug)]
pub enum ZipHunterError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Join(tokio::task::JoinError),
    Http(reqwest::Error),
    HttpStatus(reqwest::StatusCode),
    Database(sqlx::Error),
    InvalidInput(&'static str),
}

impl Display for ZipHunterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "zip hunter I/O failed: {error}"),
            Self::Zip(error) => write!(f, "zip hunter archive scan failed: {error}"),
            Self::Join(error) => write!(f, "zip hunter worker failed: {error}"),
            Self::Http(error) => write!(f, "zip hunter network request failed: {error}"),
            Self::HttpStatus(status) => {
                write!(f, "zip hunter fallback download failed with HTTP status {status}")
            }
            Self::Database(error) => write!(f, "zip hunter database update failed: {error}"),
            Self::InvalidInput(reason) => write!(f, "invalid zip hunter input: {reason}"),
        }
    }
}

impl std::error::Error for ZipHunterError {}

impl From<std::io::Error> for ZipHunterError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<zip::result::ZipError> for ZipHunterError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}

impl From<tokio::task::JoinError> for ZipHunterError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value)
    }
}

impl From<reqwest::Error> for ZipHunterError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

impl From<sqlx::Error> for ZipHunterError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

/// Scan the supplied ZIP archives for candidate memory files and stage any
/// matched `main` / `overlay` entries into `.staging/`.
pub async fn find_and_extract_memory(
    zip_paths: &[PathBuf],
    date: &str,
    mid: &str,
    media_download_url: Option<&str>,
    database_url: Option<&str>,
) -> Result<ZipMemoryScan, ZipHunterError> {
    if zip_paths.is_empty() {
        return Err(ZipHunterError::InvalidInput("zip_paths is empty"));
    }

    let normalized_date = date.trim();
    if normalized_date.is_empty() {
        return Err(ZipHunterError::InvalidInput("date is empty"));
    }

    let normalized_mid = mid.trim();
    if normalized_mid.is_empty() {
        return Err(ZipHunterError::InvalidInput("mid is empty"));
    }

    let zip_paths = zip_paths.to_vec();
    let normalized_date = normalized_date.to_string();
    let normalized_mid = normalized_mid.to_string();
    let scan_date = normalized_date.clone();
    let scan_mid = normalized_mid.clone();
    let scan_zip_paths = zip_paths.clone();
    let fallback_zip_paths = zip_paths.clone();

    let (entries, main_entry, overlay_entry, staging_dir, staged_main_path, staged_overlay_path) =
        tokio::task::spawn_blocking(move || {
            let (entries, main_entry, overlay_entry) =
                scan_zip_archives(&zip_paths, &scan_date, &scan_mid)?;
            let (staging_dir, staged_main_path, staged_overlay_path) =
                stage_matched_entries(&scan_zip_paths, main_entry.as_ref(), overlay_entry.as_ref())?;

            Ok::<_, ZipHunterError>((
                entries,
                main_entry,
                overlay_entry,
                staging_dir,
                staged_main_path,
                staged_overlay_path,
            ))
        })
        .await??;

    let mut scan = ZipMemoryScan {
        date: normalized_date,
        mid: normalized_mid,
        entries,
        main_entry,
        overlay_entry,
        staging_dir,
        staged_main_path,
        staged_overlay_path,
        used_network_fallback: false,
    };

    if scan.staged_main_path.is_none() {
        let fallback_url = media_download_url
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .ok_or(ZipHunterError::InvalidInput(
                "media_download_url is empty when zip main entry is missing",
            ))?;

        let staging_dir = scan
            .staging_dir
            .clone()
            .unwrap_or(resolve_staging_dir(&fallback_zip_paths)?);

        tokio::fs::create_dir_all(&staging_dir).await?;

        match download_main_to_staging(fallback_url, &staging_dir, &scan.date, &scan.mid).await {
            Ok(staged_main_path) => {
                scan.staging_dir = Some(staging_dir);
                scan.staged_main_path = Some(staged_main_path);
                scan.used_network_fallback = true;
            }
            Err(error) => {
                if let Some(database_url) = database_url {
                    update_failed_network_status(database_url, &scan.date, &scan.mid).await?;
                }

                return Err(error);
            }
        }
    }

    Ok(scan)
}

async fn download_main_to_staging(
    media_download_url: &str,
    staging_dir: &Path,
    date: &str,
    mid: &str,
) -> Result<PathBuf, ZipHunterError> {
    let response = reqwest::get(media_download_url).await?;
    let status = response.status();

    if !status.is_success() {
        return Err(ZipHunterError::HttpStatus(status));
    }

    let response_bytes = response.bytes().await?;
    let file_extension = infer_extension_from_download_url(media_download_url);
    let staged_file_name = match file_extension {
        Some(extension) => format!("{date}_{mid}-main.{extension}"),
        None => format!("{date}_{mid}-main"),
    };
    let staged_path = staging_dir.join(staged_file_name);

    tokio::fs::write(&staged_path, response_bytes).await?;

    Ok(staged_path)
}

fn infer_extension_from_download_url(media_download_url: &str) -> Option<String> {
    let parsed_url = url::Url::parse(media_download_url).ok()?;
    let file_name = Path::new(parsed_url.path()).file_name()?.to_str()?;
    let extension = Path::new(file_name).extension()?.to_str()?;

    Some(extension.to_ascii_lowercase())
}

async fn update_failed_network_status(
    database_url: &str,
    date: &str,
    mid: &str,
) -> Result<(), ZipHunterError> {
    let pool = SqlitePool::connect(database_url).await?;

    sqlx::query(
        "
        UPDATE Memories
        SET status = 'FAILED_NETWORK'
        WHERE date = ?1 AND mid = ?2
        ",
    )
    .bind(date)
    .bind(mid)
    .execute(&pool)
    .await?;

    pool.close().await;

    Ok(())
}

#[allow(clippy::type_complexity)]
fn scan_zip_archives(
    zip_paths: &[PathBuf],
    date: &str,
    mid: &str,
) -> Result<
    (
        Vec<ZipArchiveEntry>,
        Option<ZipArchiveEntry>,
        Option<ZipArchiveEntry>,
    ),
    ZipHunterError,
> {
    let mut entries = Vec::new();
    let mut main_entry = None;
    let mut overlay_entry = None;

    for zip_path in zip_paths {
        let file = std::fs::File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        collect_entries_from_archive(
            zip_path,
            &mut archive,
            date,
            mid,
            &mut entries,
            &mut main_entry,
            &mut overlay_entry,
        )?;
    }

    Ok((entries, main_entry, overlay_entry))
}

fn collect_entries_from_archive<R: std::io::Read + std::io::Seek>(
    zip_path: &Path,
    archive: &mut zip::ZipArchive<R>,
    date: &str,
    mid: &str,
    entries: &mut Vec<ZipArchiveEntry>,
    main_entry: &mut Option<ZipArchiveEntry>,
    overlay_entry: &mut Option<ZipArchiveEntry>,
) -> Result<(), ZipHunterError> {
    for entry_index in 0..archive.len() {
        let file = archive.by_index(entry_index)?;

        if file.is_dir() {
            continue;
        }

        let entry = ZipArchiveEntry {
            zip_path: zip_path.to_path_buf(),
            entry_index,
            entry_name: file.name().to_string(),
            compressed_size: file.compressed_size(),
            uncompressed_size: file.size(),
        };

        match match_memory_entry_kind(&entry.entry_name, date, mid) {
            Some(MemoryEntryKind::Main) if main_entry.is_none() => {
                *main_entry = Some(entry.clone());
            }
            Some(MemoryEntryKind::Overlay) if overlay_entry.is_none() => {
                *overlay_entry = Some(entry.clone());
            }
            _ => {}
        }

        entries.push(entry);
    }

    Ok(())
}

fn match_memory_entry_kind(entry_name: &str, date: &str, mid: &str) -> Option<MemoryEntryKind> {
    let file_name = Path::new(entry_name)
        .file_name()
        .and_then(|value| value.to_str())?;

    let main_pattern = format!("{date}_{mid}-main");
    if file_name.contains(&main_pattern) {
        return Some(MemoryEntryKind::Main);
    }

    let overlay_pattern = format!("{date}_{mid}-overlay");
    if file_name.contains(&overlay_pattern) {
        return Some(MemoryEntryKind::Overlay);
    }

    None
}

#[allow(clippy::type_complexity)]
fn stage_matched_entries(
    zip_paths: &[PathBuf],
    main_entry: Option<&ZipArchiveEntry>,
    overlay_entry: Option<&ZipArchiveEntry>,
) -> Result<(Option<PathBuf>, Option<PathBuf>, Option<PathBuf>), ZipHunterError> {
    if main_entry.is_none() && overlay_entry.is_none() {
        return Ok((None, None, None));
    }

    let staging_dir = resolve_staging_dir(zip_paths)?;
    std::fs::create_dir_all(&staging_dir)?;

    let staged_main_path = if let Some(entry) = main_entry {
        Some(extract_entry_to_staging(entry, &staging_dir)?)
    } else {
        None
    };

    let staged_overlay_path = if let Some(entry) = overlay_entry {
        Some(extract_entry_to_staging(entry, &staging_dir)?)
    } else {
        None
    };

    Ok((Some(staging_dir), staged_main_path, staged_overlay_path))
}

fn resolve_staging_dir(zip_paths: &[PathBuf]) -> Result<PathBuf, ZipHunterError> {
    let first_zip_path = zip_paths
        .first()
        .ok_or(ZipHunterError::InvalidInput("zip_paths is empty"))?;

    let base_dir = first_zip_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    Ok(base_dir.join(".staging"))
}

fn extract_entry_to_staging(
    entry: &ZipArchiveEntry,
    staging_dir: &Path,
) -> Result<PathBuf, ZipHunterError> {
    let file = std::fs::File::open(&entry.zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut zip_file = archive.by_index(entry.entry_index)?;

    let staged_file_name = Path::new(&entry.entry_name)
        .file_name()
        .ok_or(ZipHunterError::InvalidInput("matched zip entry has no file name"))?;
    let staged_path = staging_dir.join(staged_file_name);
    let mut staged_file = std::fs::File::create(&staged_path)?;
    std::io::copy(&mut zip_file, &mut staged_file)?;

    Ok(staged_path)
}

#[cfg(test)]
mod tests {
    use super::{find_and_extract_memory, ZipHunterError};
    use std::fs::File;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    fn start_single_response_http_server(
        status_line: &'static str,
        body: &'static [u8],
        content_type: &'static str,
    ) -> Result<(String, thread::JoinHandle<()>), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let url = format!("http://{}/download.mp4", address);

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("http test server should accept one connection");

            let mut request_buffer = [0_u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut request_buffer)
                .expect("http test server should read request bytes");

            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
                body.len()
            );

            stream
                .write_all(response.as_bytes())
                .expect("http test server should write headers");
            stream
                .write_all(body)
                .expect("http test server should write body");
        });

        Ok((url, handle))
    }

    async fn create_memories_table(database_url: &str) {
        let pool = sqlx::SqlitePool::connect(database_url)
            .await
            .expect("sqlite pool should be created for zip hunter tests");

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS Memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hash TEXT NOT NULL UNIQUE,
                date TEXT NOT NULL,
                status TEXT NOT NULL,
                job_id TEXT,
                mid TEXT,
                content_hash TEXT,
                relative_path TEXT,
                thumbnail_path TEXT
            )
            ",
        )
        .execute(&pool)
        .await
        .expect("memories table should be created for zip hunter tests");

        pool.close().await;
    }

    fn write_zip_fixture(
        zip_path: &std::path::Path,
        entries: &[(&str, &[u8])],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let file = File::create(zip_path)?;
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, contents) in entries {
            writer.start_file(name, options)?;
            writer.write_all(contents)?;
        }

        writer.finish()?;
        Ok(())
    }

    #[tokio::test]
    async fn scans_entries_and_extracts_only_matched_files_into_staging() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for zip hunter tests");
        let first_zip_path = temp_dir.path().join("part-1.zip");
        let second_zip_path = temp_dir.path().join("part-2.zip");

        write_zip_fixture(
            &first_zip_path,
            &[
                ("exports/2026-02-20_alpha-main.mp4", b"video-data"),
                ("exports/readme.txt", b"hello"),
            ],
        )
        .expect("first zip fixture should be written");

        write_zip_fixture(
            &second_zip_path,
            &[("exports/2026-02-20_alpha-overlay.png", b"overlay-data")],
        )
        .expect("second zip fixture should be written");

        let scan = find_and_extract_memory(
            &[first_zip_path.clone(), second_zip_path.clone()],
            "2026-02-20",
            "alpha",
            None,
            None,
        )
        .await
        .expect("zip scan should succeed");

        assert_eq!(scan.date, "2026-02-20");
        assert_eq!(scan.mid, "alpha");
        assert_eq!(scan.entries.len(), 3);
        assert_eq!(
            scan.main_entry.as_ref().map(|entry| entry.entry_name.as_str()),
            Some("exports/2026-02-20_alpha-main.mp4")
        );
        assert_eq!(
            scan.overlay_entry
                .as_ref()
                .map(|entry| entry.entry_name.as_str()),
            Some("exports/2026-02-20_alpha-overlay.png")
        );
        assert_eq!(
            scan.staged_main_path.as_deref(),
            Some(temp_dir.path().join(".staging/2026-02-20_alpha-main.mp4").as_path())
        );
        assert_eq!(
            scan.staged_overlay_path.as_deref(),
            Some(temp_dir.path().join(".staging/2026-02-20_alpha-overlay.png").as_path())
        );

        assert!(scan.entries.iter().any(|entry| {
            entry.zip_path == first_zip_path
                && entry.entry_name == "exports/2026-02-20_alpha-main.mp4"
        }));
        assert!(scan.entries.iter().any(|entry| {
            entry.zip_path == second_zip_path
                && entry.entry_name == "exports/2026-02-20_alpha-overlay.png"
        }));

        let extracted_main_path = temp_dir.path().join(".staging/2026-02-20_alpha-main.mp4");
        assert!(
            extracted_main_path.exists(),
            "matched main file should be extracted into .staging"
        );

        let extracted_overlay_path = temp_dir.path().join(".staging/2026-02-20_alpha-overlay.png");
        assert!(
            extracted_overlay_path.exists(),
            "matched overlay file should be extracted into .staging"
        );

        let unrelated_path = temp_dir.path().join(".staging/readme.txt");
        assert!(
            !unrelated_path.exists(),
            "unmatched files should not be extracted into .staging"
        );
        assert!(!scan.used_network_fallback);
    }

    #[tokio::test]
    async fn rejects_empty_zip_path_lists() {
        let error = find_and_extract_memory(&[], "2026-02-20", "alpha", None, None)
            .await
            .expect_err("empty zip path list should be rejected");

        assert!(matches!(error, ZipHunterError::InvalidInput("zip_paths is empty")));
    }

    #[tokio::test]
    async fn matches_only_entries_for_the_requested_date_and_mid() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for zip hunter tests");
        let zip_path = temp_dir.path().join("part-1.zip");

        write_zip_fixture(
            &zip_path,
            &[
                ("exports/2026-02-20_alpha-main.mp4", b"video-data"),
                ("exports/2026-02-20_alpha-overlay.png", b"overlay-data"),
                ("exports/2026-02-20_beta-main.mp4", b"other-mid"),
                ("exports/2026-02-19_alpha-main.mp4", b"other-date"),
            ],
        )
        .expect("zip fixture should be written");

        let scan = find_and_extract_memory(&[zip_path], "2026-02-20", "alpha", None, None)
            .await
            .expect("zip scan should succeed");

        assert_eq!(
            scan.main_entry.as_ref().map(|entry| entry.entry_name.as_str()),
            Some("exports/2026-02-20_alpha-main.mp4")
        );
        assert_eq!(
            scan.overlay_entry
                .as_ref()
                .map(|entry| entry.entry_name.as_str()),
            Some("exports/2026-02-20_alpha-overlay.png")
        );
    }

    #[tokio::test]
    async fn verification_extracts_only_the_target_main_file_into_staging() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for zip hunter tests");
        let zip_path = temp_dir.path().join("part-1.zip");

        write_zip_fixture(
            &zip_path,
            &[
                ("exports/2026-02-20_9a5a-main.mp4", b"target-video"),
                ("exports/2026-02-20_7b7b-main.mp4", b"other-video"),
                ("exports/notes.txt", b"ignore-me"),
            ],
        )
        .expect("zip fixture should be written");

        let scan = find_and_extract_memory(&[zip_path], "2026-02-20", "9a5a", None, None)
            .await
            .expect("zip scan should succeed");

        let staging_dir = scan
            .staging_dir
            .as_ref()
            .expect("staging directory should be present when a match is found");
        let staged_files = std::fs::read_dir(staging_dir)
            .expect("staging directory should be readable")
            .map(|entry| {
                entry
                    .expect("staging directory entry should be readable")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert_eq!(staged_files, vec!["2026-02-20_9a5a-main.mp4"]);
    }

    #[tokio::test]
    async fn downloads_main_file_into_staging_when_zip_match_is_missing() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for zip hunter tests");
        let zip_path = temp_dir.path().join("part-1.zip");
        write_zip_fixture(&zip_path, &[("exports/2026-02-20_alpha-overlay.png", b"overlay")])
            .expect("zip fixture should be written");

        let (download_url, server_handle) = start_single_response_http_server(
            "200 OK",
            b"downloaded-video",
            "video/mp4",
        )
        .expect("http server should start");

        let scan = find_and_extract_memory(
            &[zip_path],
            "2026-02-20",
            "alpha",
            Some(&download_url),
            None,
        )
        .await
        .expect("fallback download should succeed");

        server_handle
            .join()
            .expect("http server thread should complete successfully");

        let staged_main_path = scan
            .staged_main_path
            .as_ref()
            .expect("downloaded main file should be staged");
        assert_eq!(
            staged_main_path.file_name().and_then(|name| name.to_str()),
            Some("2026-02-20_alpha-main.mp4")
        );
        assert_eq!(
            std::fs::read(staged_main_path).expect("downloaded staged file should be readable"),
            b"downloaded-video"
        );
        assert!(scan.used_network_fallback);
    }

    #[tokio::test]
    async fn marks_memory_as_failed_network_when_fallback_download_fails() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created for zip hunter tests");
        let zip_path = temp_dir.path().join("part-1.zip");
        write_zip_fixture(&zip_path, &[("exports/readme.txt", b"hello")])
            .expect("zip fixture should be written");

        let db_path = temp_dir.path().join("memories.db");
        let database_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());
        create_memories_table(&database_url).await;

        let pool = sqlx::SqlitePool::connect(&database_url)
            .await
            .expect("sqlite pool should be created for verification");

        sqlx::query(
            "
            INSERT INTO Memories (hash, date, status, mid)
            VALUES (?1, ?2, ?3, ?4)
            ",
        )
        .bind("hash-1")
        .bind("2026-02-20")
        .bind("queued")
        .bind("alpha")
        .execute(&pool)
        .await
        .expect("memory row should be inserted");

        pool.close().await;

        let (download_url, server_handle) = start_single_response_http_server(
            "500 Internal Server Error",
            b"boom",
            "text/plain",
        )
        .expect("http server should start");

        let error = find_and_extract_memory(
            &[zip_path],
            "2026-02-20",
            "alpha",
            Some(&download_url),
            Some(&database_url),
        )
        .await
        .expect_err("failed fallback download should return an error");

        server_handle
            .join()
            .expect("http server thread should complete successfully");

        assert!(matches!(error, ZipHunterError::HttpStatus(reqwest::StatusCode::INTERNAL_SERVER_ERROR)));

        let verification_pool = sqlx::SqlitePool::connect(&database_url)
            .await
            .expect("sqlite pool should be opened for status verification");

        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM Memories WHERE date = ?1 AND mid = ?2 LIMIT 1",
        )
        .bind("2026-02-20")
        .bind("alpha")
        .fetch_one(&verification_pool)
        .await
        .expect("status query should succeed");

        assert_eq!(status, "FAILED_NETWORK");

        verification_pool.close().await;
    }
}