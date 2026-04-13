use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{NaiveDate, NaiveDateTime};
use sqlx::SqlitePool;

use crate::core::media;

const FFMPEG_TIMEOUT_THUMBNAIL_SECS: u64 = 30;
const FFMPEG_POLL_INTERVAL_MS: u64 = 200;

#[derive(Debug)]
pub enum ProcessorError {
    Io(std::io::Error),
    Join(tokio::task::JoinError),
    Media(media::MediaError),
    Database(sqlx::Error),
    InvalidInput(&'static str),
    FfmpegFailed { status: Option<i32>, stderr: String },
    Blake3(String),
}

impl Display for ProcessorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "processor I/O failed: {error}"),
            Self::Join(error) => write!(f, "processor thread failed: {error}"),
            Self::Media(error) => write!(f, "media operation failed: {error}"),
            Self::Database(error) => write!(f, "processor DB error: {error}"),
            Self::InvalidInput(reason) => write!(f, "invalid processor input: {reason}"),
            Self::FfmpegFailed { status, stderr } => {
                write!(
                    f,
                    "ffmpeg exited with status {:?}: {}",
                    status,
                    stderr.trim()
                )
            }
            Self::Blake3(msg) => write!(f, "blake3 hashing failed: {msg}"),
        }
    }
}

impl std::error::Error for ProcessorError {}

impl From<std::io::Error> for ProcessorError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tokio::task::JoinError> for ProcessorError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value)
    }
}

impl From<media::MediaError> for ProcessorError {
    fn from(value: media::MediaError) -> Self {
        Self::Media(value)
    }
}

impl From<sqlx::Error> for ProcessorError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

#[derive(Debug)]
pub enum ProcessMediaResult {
    Processed(ProcessMediaOutput),
    Duplicate { content_hash: String },
}

#[derive(Debug, Clone, Copy)]
pub enum ThumbnailQuality {
    P360,
    P480,
    P720,
    P1080,
}

impl ThumbnailQuality {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("360p") => Self::P360,
            Some("720p") => Self::P720,
            Some("1080p") => Self::P1080,
            Some("480p") | None | Some(_) => Self::P480,
        }
    }

    pub fn max_dimension(self) -> u16 {
        match self {
            Self::P360 => 360,
            Self::P480 => 480,
            Self::P720 => 720,
            Self::P1080 => 1080,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessMediaInput {
    pub memory_item_id: i64,
    pub memory_group_id: Option<i64>,
    pub raw_media_paths: Vec<PathBuf>,
    pub overlay_path: Option<PathBuf>,
    pub date_taken: String,
    pub location: Option<String>,
    pub export_dir: PathBuf,
    pub thumbnail_dir: PathBuf,
    pub thumbnail_max_dimension: u16,
    pub video_output_profile: media::VideoOutputProfile,
    pub image_output_format: media::ImageOutputFormat,
    pub image_quality: media::ImageQuality,
    pub hw_accel: media::HwAccelPreference,
    pub overlay_strategy: media::OverlayStrategy,
    pub keep_originals: bool,
    pub database_url: String,
}

#[derive(Debug, Clone)]
pub struct ProcessMediaOutput {
    pub final_media_path: PathBuf,
    pub thumbnail_path: PathBuf,
    pub content_hash: String,
    pub overlay_requested: bool,
    pub overlay_applied: bool,
    pub overlay_fallback_reason: Option<String>,
}

pub async fn process_media(input: ProcessMediaInput) -> Result<ProcessMediaResult, ProcessorError> {
    if input.raw_media_paths.is_empty() {
        return Err(ProcessorError::InvalidInput("raw_media_paths is empty"));
    }

    let content_hash = compute_blake3_hash(&input.raw_media_paths[0]).await?;
    let duplicate_row_id = input.memory_group_id.unwrap_or(input.memory_item_id);

    if !input.database_url.is_empty()
        && check_duplicate_in_db(&content_hash, duplicate_row_id, &input.database_url).await?
    {
        cleanup_source_artifacts(
            &input.raw_media_paths,
            input.overlay_path.as_deref(),
            None,
            input.keep_originals,
        )
        .await?;

        eprintln!(
            "[processor-debug] duplicate detected memory_group_id={:?} memory_item_id={} hash={content_hash}",
            input.memory_group_id,
            input.memory_item_id
        );

        return Ok(ProcessMediaResult::Duplicate { content_hash });
    }

    eprintln!(
        "[processor-debug] process_media start memory_item_id={} raw_count={} overlay_present={} export_dir='{}' keep_originals={}",
        input.memory_item_id,
        input.raw_media_paths.len(),
        input.overlay_path.is_some(),
        input.export_dir.display(),
        input.keep_originals
    );

    let source_extension = media_extension_from_path(&input.raw_media_paths[0]).unwrap_or("bin");
    let source_is_video = is_video_extension(source_extension);
    let output_extension = if source_is_video {
        input.video_output_profile.output_extension()
    } else {
        input.image_output_format.output_extension()
    };
    let final_media_path = build_final_media_path(
        &input.export_dir,
        &input.date_taken,
        input.memory_item_id,
        output_extension,
    )?;
    let thumbnail_path = build_thumbnail_path(&input.thumbnail_dir, input.memory_item_id);

    if let Some(parent) = final_media_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(parent) = thumbnail_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut temp_concat_path: Option<PathBuf> = None;

    if input.raw_media_paths.len() > 1 && source_is_video {
        let concat_output_path = input.export_dir.join(format!(
            "{}.concat.{}",
            input.memory_item_id, source_extension
        ));

        eprintln!(
            "[processor-debug] concatenating parts memory_item_id={} parts={} concat_output='{}'",
            input.memory_item_id,
            input.raw_media_paths.len(),
            concat_output_path.display()
        );

        concat_video_parts(&input.raw_media_paths, &concat_output_path).await?;
        temp_concat_path = Some(concat_output_path);
    }

    let base_media_path = temp_concat_path
        .as_deref()
        .or_else(|| input.raw_media_paths.first().map(PathBuf::as_path))
        .ok_or(ProcessorError::InvalidInput("missing base media path"))?;

    let mut effective_overlay_path = input.overlay_path.as_deref();
    let overlay_requested = input.overlay_path.is_some();
    let mut overlay_applied = false;
    let mut overlay_fallback_reason: Option<String> = None;
    let encoding_options = media::MediaEncodingOptions {
        video_profile: input.video_output_profile,
        image_format: input.image_output_format,
        image_quality: input.image_quality,
        hw_accel: input.hw_accel,
        overlay_strategy: input.overlay_strategy,
    };

    let merge_result = merge_staged_media(
        base_media_path,
        effective_overlay_path,
        &final_media_path,
        encoding_options,
    )
    .await;

    if let Err(error) = merge_result {
        // If hw accel was enabled, retry with software encoding first.
        let hw_was_enabled = !matches!(
            encoding_options.hw_accel,
            media::HwAccelPreference::Disabled
        ) && encoding_options.hw_accel.resolve().is_some();
        if hw_was_enabled {
            eprintln!(
                "[processor-debug] hw-accelerated merge failed for memory_item_id={}, retrying with software: {}",
                input.memory_item_id, error
            );
            let sw_options = media::MediaEncodingOptions {
                hw_accel: media::HwAccelPreference::Disabled,
                ..encoding_options
            };
            let sw_result = merge_staged_media(
                base_media_path,
                effective_overlay_path,
                &final_media_path,
                sw_options,
            )
            .await;
            match sw_result {
                Ok(()) => {
                    if overlay_requested {
                        overlay_applied = effective_overlay_path.is_some();
                    }
                }
                Err(sw_err) if effective_overlay_path.is_some() => {
                    let fallback_msg = sw_err.to_string();
                    overlay_fallback_reason = Some(fallback_msg.clone());
                    overlay_applied = false;
                    eprintln!(
                        "[processor-debug] software overlay merge also failed for memory_item_id={}, retrying without overlay: {}",
                        input.memory_item_id, fallback_msg
                    );
                    effective_overlay_path = None;
                    merge_staged_media(base_media_path, None, &final_media_path, sw_options)
                        .await?;
                }
                Err(sw_err) => {
                    return Err(sw_err);
                }
            }
        } else if effective_overlay_path.is_some() {
            let fallback_msg = error.to_string();
            overlay_fallback_reason = Some(fallback_msg.clone());
            overlay_applied = false;
            eprintln!(
                "[processor-debug] overlay merge failed for memory_item_id={}, retrying without overlay: {}",
                input.memory_item_id, fallback_msg
            );

            effective_overlay_path = None;
            merge_staged_media(base_media_path, None, &final_media_path, encoding_options).await?;
        } else {
            return Err(error);
        }
    } else if overlay_requested {
        overlay_applied = true;
    }

    // Post-transcode integrity check for videos: verify codec and pixel format.
    // If the chosen profile produces a broken output, retry with Mp4Compatible.
    let final_media_path = if source_is_video {
        let is_valid = media::verify_video_integrity(&final_media_path, input.video_output_profile)
            .await
            .unwrap_or(false);

        if !is_valid
            && !matches!(
                input.video_output_profile,
                media::VideoOutputProfile::Mp4Compatible
            )
        {
            eprintln!(
                "[processor-debug] integrity check failed for memory_item_id={}, retrying with Mp4Compatible",
                input.memory_item_id
            );

            let fallback_path = final_media_path.with_extension("mp4");
            merge_staged_media(
                base_media_path,
                effective_overlay_path,
                &fallback_path,
                media::MediaEncodingOptions {
                    video_profile: media::VideoOutputProfile::Mp4Compatible,
                    image_format: input.image_output_format,
                    image_quality: input.image_quality,
                    hw_accel: input.hw_accel,
                    overlay_strategy: input.overlay_strategy,
                },
            )
            .await?;

            if fallback_path != final_media_path {
                let _ = tokio::fs::remove_file(&final_media_path).await;
            }

            fallback_path
        } else {
            final_media_path
        }
    } else {
        final_media_path
    };

    media::write_metadata_with_ffmpeg(
        &final_media_path,
        &input.date_taken,
        input.location.as_deref(),
    )
    .await?;

    generate_webp_thumbnail(
        &final_media_path,
        &thumbnail_path,
        input.thumbnail_max_dimension,
    )
    .await?;

    eprintln!(
        "[processor-debug] process_media success memory_item_id={} final_media='{}' thumbnail='{}' overlay_requested={} overlay_applied={} overlay_fallback={}",
        input.memory_item_id,
        final_media_path.display(),
        thumbnail_path.display(),
        overlay_requested,
        overlay_applied,
        overlay_fallback_reason.is_some()
    );

    cleanup_source_artifacts(
        &input.raw_media_paths,
        input.overlay_path.as_deref(),
        temp_concat_path.as_deref(),
        input.keep_originals,
    )
    .await?;

    Ok(ProcessMediaResult::Processed(ProcessMediaOutput {
        final_media_path,
        thumbnail_path,
        content_hash,
        overlay_requested,
        overlay_applied,
        overlay_fallback_reason,
    }))
}

pub async fn check_duplicate_in_db(
    content_hash: &str,
    memory_group_id: i64,
    database_url: &str,
) -> Result<bool, ProcessorError> {
    let pool = SqlitePool::connect(database_url).await?;

    let duplicate_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM Memories WHERE content_hash = ?1 AND id != ?2 LIMIT 1")
            .bind(content_hash)
            .bind(memory_group_id)
            .fetch_optional(&pool)
            .await?;

    if duplicate_id.is_some() {
        sqlx::query("UPDATE Memories SET status = 'DUPLICATE' WHERE id = ?1")
            .bind(memory_group_id)
            .execute(&pool)
            .await?;

        return Ok(true);
    }

    Ok(false)
}

pub async fn compute_blake3_hash(path: &Path) -> Result<String, ProcessorError> {
    use std::io::Read;

    let path = path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let mut hasher = blake3::Hasher::new();
        let file = std::fs::File::open(&path)
            .map_err(|e| ProcessorError::Blake3(format!("open '{}': {e}", path.display())))?;
        let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
        let mut buf = [0u8; 64 * 1024];

        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| ProcessorError::Blake3(format!("read '{}': {e}", path.display())))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }

        Ok(hasher.finalize().to_hex().to_string())
    })
    .await?
}

fn build_final_media_path(
    export_root: &Path,
    date_taken: &str,
    memory_item_id: i64,
    extension: &str,
) -> Result<PathBuf, ProcessorError> {
    let capture_date = parse_capture_date(date_taken)?;
    let year = capture_date.format("%Y").to_string();
    let month_dir = capture_date.format("%m_%B").to_string();

    Ok(export_root
        .join(year)
        .join(month_dir)
        .join(format!("{}.{}", memory_item_id, extension)))
}

fn build_thumbnail_path(thumbnail_root: &Path, memory_item_id: i64) -> PathBuf {
    thumbnail_root.join(format!("{}.webp", memory_item_id))
}

fn parse_capture_date(date_taken: &str) -> Result<NaiveDate, ProcessorError> {
    if let Ok(date) = NaiveDate::parse_from_str(date_taken, "%Y-%m-%d") {
        return Ok(date);
    }

    if let Ok(date_time) = chrono::DateTime::parse_from_rfc3339(date_taken) {
        return Ok(date_time.date_naive());
    }

    if let Ok(date_time) = NaiveDateTime::parse_from_str(date_taken, "%Y-%m-%d %H:%M:%S") {
        return Ok(date_time.date());
    }

    Err(ProcessorError::InvalidInput(
        "date_taken is not a supported date format",
    ))
}

async fn merge_staged_media(
    base_media_path: &Path,
    overlay_path: Option<&Path>,
    output_path: &Path,
    encoding_options: media::MediaEncodingOptions,
) -> Result<(), ProcessorError> {
    let existing_overlay_path = resolve_existing_overlay_path(overlay_path).await?;

    // Probe base media and overlay for resolution-aware composition.
    let base_probe = match media::probe_media(base_media_path).await {
        Ok(probe) => Some(probe),
        Err(e) => {
            eprintln!(
                "[processor-debug] probe failed for base='{}': {e}",
                base_media_path.display()
            );
            None
        }
    };

    let overlay_probe = if let Some(ov_path) = existing_overlay_path.as_deref() {
        match media::probe_image(ov_path).await {
            Ok(probe) => Some(probe),
            Err(e) => {
                eprintln!(
                    "[processor-debug] probe failed for overlay='{}': {e}",
                    ov_path.display()
                );
                None
            }
        }
    } else {
        None
    };

    if let Some(overlay_path) = existing_overlay_path.as_deref() {
        eprintln!(
            "[processor-debug] applying overlay base='{}' overlay='{}' output='{}' \
             base_dims={}x{} overlay_dims={}x{} rotation={}",
            base_media_path.display(),
            overlay_path.display(),
            output_path.display(),
            base_probe.as_ref().map_or(0, |p| p.display_width),
            base_probe.as_ref().map_or(0, |p| p.display_height),
            overlay_probe.as_ref().map_or(0, |p| p.width),
            overlay_probe.as_ref().map_or(0, |p| p.height),
            base_probe.as_ref().map_or(0, |p| p.rotation),
        );
    } else {
        eprintln!(
            "[processor-debug] no overlay found; copying base='{}' output='{}'",
            base_media_path.display(),
            output_path.display()
        );
    }

    media::merge_media_with_optional_overlay(
        base_media_path,
        existing_overlay_path.as_deref(),
        output_path,
        encoding_options,
        base_probe.as_ref(),
        overlay_probe.as_ref(),
    )
    .await?;

    Ok(())
}

async fn resolve_existing_overlay_path(
    overlay_path: Option<&Path>,
) -> Result<Option<PathBuf>, ProcessorError> {
    let Some(overlay_path) = overlay_path else {
        return Ok(None);
    };

    if tokio::fs::try_exists(overlay_path).await? {
        return Ok(Some(overlay_path.to_path_buf()));
    }

    Ok(None)
}

async fn concat_video_parts(parts: &[PathBuf], output_path: &Path) -> Result<(), ProcessorError> {
    let parts = parts.to_vec();
    let output_path = output_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let list_path = output_path.with_extension("concat.txt");
        let list_path_for_ffmpeg =
            std::fs::canonicalize(&list_path).unwrap_or_else(|_| list_path.clone());
        let mut list_content = String::new();

        for part in &parts {
            let absolute_part_path = std::fs::canonicalize(part).unwrap_or_else(|_| part.clone());
            list_content.push_str(&format!(
                "file '{}'\n",
                escape_ffmpeg_concat_entry_path(&absolute_part_path)
            ));
        }

        std::fs::write(&list_path, list_content)?;

        eprintln!(
            "[processor-debug] ffmpeg concat start parts={} output='{}'",
            parts.len(),
            output_path.display()
        );

        let args = vec![
            "-y".to_string(),
            "-f".to_string(),
            "concat".to_string(),
            "-safe".to_string(),
            "0".to_string(),
            "-i".to_string(),
            list_path_for_ffmpeg.to_string_lossy().to_string(),
            "-c".to_string(),
            "copy".to_string(),
            output_path.to_string_lossy().to_string(),
        ];
        let concat_result = run_ffmpeg_with_timeout(&args, FFMPEG_TIMEOUT_THUMBNAIL_SECS);

        let _ = std::fs::remove_file(&list_path);
        concat_result?;
        Ok(())
    })
    .await?
}

fn escape_ffmpeg_concat_entry_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''")
}

async fn generate_webp_thumbnail(
    media_path: &Path,
    thumbnail_path: &Path,
    thumbnail_max_dimension: u16,
) -> Result<(), ProcessorError> {
    let media_path = media_path.to_path_buf();
    let thumbnail_path = thumbnail_path.to_path_buf();
    let media_extension = media_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let is_video = is_video_extension(&media_extension);

    tokio::task::spawn_blocking(move || {
        if let Some(parent) = thumbnail_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        eprintln!(
            "[processor-debug] thumbnail generation start media='{}' thumbnail='{}'",
            media_path.display(),
            thumbnail_path.display()
        );

        let media_path_arg = media_path.to_string_lossy().to_string();
        let thumbnail_path_arg = thumbnail_path.to_string_lossy().to_string();
        let mut args: Vec<String> = Vec::new();

        if is_video {
            // Seek to 1 second before the end so the thumbnail captures the
            // overlay text/graphics that are composited onto every frame.
            // -sseof must come before -i to be an input option.
            args.push("-y".to_string());
            args.push("-sseof".to_string());
            args.push("-1".to_string());
            args.push("-i".to_string());
            args.push(media_path_arg);
            args.push("-map".to_string());
            args.push("0:v:0".to_string());
            args.push("-vf".to_string());
            let scale_filter = format!(
                "crop=iw-2:ih-2:1:1,scale={0}:{0}:force_original_aspect_ratio=decrease:flags=lanczos",
                thumbnail_max_dimension
            );
            args.push(scale_filter);
        } else {
            args.push("-y".to_string());
            args.push("-i".to_string());
            args.push(media_path_arg);
            args.push("-vf".to_string());
            let scale_filter = format!(
                "crop=iw-2:ih-2:1:1,scale={0}:{0}:force_original_aspect_ratio=decrease:flags=lanczos",
                thumbnail_max_dimension
            );
            args.push(scale_filter);
        }

        args.push("-frames:v".to_string());
        args.push("1".to_string());
        args.push(thumbnail_path_arg);

        run_ffmpeg_with_timeout(&args, FFMPEG_TIMEOUT_THUMBNAIL_SECS)
    })
    .await?
}

fn run_ffmpeg_with_timeout(args: &[String], timeout_secs: u64) -> Result<(), ProcessorError> {
    let stderr_log_path = std::env::temp_dir().join(format!(
        "memorysnaper-ffmpeg-{}-{}.log",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let stderr_log_file = std::fs::File::create(&stderr_log_path).map_err(ProcessorError::Io)?;
    let stderr_for_child = stderr_log_file.try_clone().map_err(ProcessorError::Io)?;

    let mut ffmpeg_args = vec![
        "-hide_banner".to_string(),
        "-nostdin".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
    ];
    ffmpeg_args.extend(args.iter().cloned());

    let mut child = Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_for_child))
        .spawn()
        .map_err(ProcessorError::Io)?;

    let started_at = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(ProcessorError::Io)? {
            break status;
        }

        if started_at.elapsed() >= Duration::from_secs(timeout_secs) {
            timed_out = true;
            let _ = child.kill();
            break child.wait().map_err(ProcessorError::Io)?;
        }

        std::thread::sleep(Duration::from_millis(FFMPEG_POLL_INTERVAL_MS));
    };

    let stderr_text = std::fs::read_to_string(&stderr_log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&stderr_log_path);

    if timed_out {
        return Err(ProcessorError::FfmpegFailed {
            status: None,
            stderr: format!(
                "ffmpeg timed out after {}s{}{}",
                timeout_secs,
                if stderr_text.trim().is_empty() {
                    ""
                } else {
                    "; stderr: "
                },
                stderr_text.trim()
            ),
        });
    }

    if status.success() {
        return Ok(());
    }

    Err(ProcessorError::FfmpegFailed {
        status: status.code(),
        stderr: stderr_text,
    })
}

async fn remove_file_if_exists(path: &Path) -> Result<(), ProcessorError> {
    if tokio::fs::try_exists(path).await? {
        tokio::fs::remove_file(path).await?;
    }

    Ok(())
}

async fn cleanup_source_artifacts(
    raw_media_paths: &[PathBuf],
    overlay_path: Option<&Path>,
    temp_concat_path: Option<&Path>,
    keep_originals: bool,
) -> Result<(), ProcessorError> {
    for raw_media_path in raw_media_paths {
        if !keep_originals || is_staging_path(raw_media_path) {
            remove_file_if_exists(raw_media_path).await?;
            remove_empty_staging_dirs(raw_media_path).await?;
        }
    }

    if let Some(overlay_path) = overlay_path {
        if !keep_originals || is_staging_path(overlay_path) {
            remove_file_if_exists(overlay_path).await?;
            remove_empty_staging_dirs(overlay_path).await?;
        }
    }

    if let Some(temp_concat_path) = temp_concat_path {
        remove_file_if_exists(temp_concat_path).await?;
        remove_empty_staging_dirs(temp_concat_path).await?;
    }

    Ok(())
}

fn is_staging_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == ".staging")
}

async fn remove_empty_staging_dirs(path: &Path) -> Result<(), ProcessorError> {
    let mut current = path.parent();

    while let Some(dir) = current {
        if dir.file_name().is_some_and(|name| name == ".staging") {
            if tokio::fs::try_exists(dir).await? {
                match tokio::fs::remove_dir(dir).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(ProcessorError::Io(error)),
                }
            }
            break;
        }

        current = dir.parent();
    }

    Ok(())
}

fn media_extension_from_path(path: &Path) -> Option<&str> {
    path.extension().and_then(|value| value.to_str())
}

fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "mp4" | "mov" | "m4v" | "webm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{tempdir, NamedTempFile};

    #[tokio::test]
    async fn blake3_same_content_produces_same_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"snapchat memory test content").unwrap();
        file.flush().unwrap();

        let hash_a = compute_blake3_hash(file.path()).await.unwrap();
        let hash_b = compute_blake3_hash(file.path()).await.unwrap();

        assert_eq!(hash_a, hash_b, "same file must yield identical hash");
    }

    #[tokio::test]
    async fn blake3_different_content_produces_different_hash() {
        let mut file_a = NamedTempFile::new().unwrap();
        file_a.write_all(b"content alpha").unwrap();

        let mut file_b = NamedTempFile::new().unwrap();
        file_b.write_all(b"content beta").unwrap();

        let hash_a = compute_blake3_hash(file_a.path()).await.unwrap();
        let hash_b = compute_blake3_hash(file_b.path()).await.unwrap();

        assert_ne!(
            hash_a, hash_b,
            "different content must yield different hashes"
        );
    }

    #[tokio::test]
    async fn blake3_hash_is_64_hex_chars() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"some media bytes").unwrap();

        let hash = compute_blake3_hash(file.path()).await.unwrap();

        assert_eq!(hash.len(), 64, "BLAKE3 hex output must be 64 characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must contain only hex characters"
        );
    }

    #[tokio::test]
    async fn blake3_missing_file_returns_error() {
        let result = compute_blake3_hash(Path::new("/nonexistent/path/file.mp4")).await;
        assert!(
            result.is_err(),
            "hashing a missing file must return an error"
        );
        assert!(matches!(result.unwrap_err(), ProcessorError::Blake3(_)));
    }

    async fn create_test_db() -> (SqlitePool, String) {
        let db_file = NamedTempFile::new().unwrap();
        let db_path = db_file.into_temp_path().keep().unwrap();
        let database_url = format!("sqlite://{}", db_path.display());
        let pool = SqlitePool::connect(&database_url).await.unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS Memories (
                id INTEGER PRIMARY KEY,
                content_hash TEXT,
                status TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        (pool, database_url)
    }

    #[tokio::test]
    async fn duplicate_check_returns_true_and_marks_db_when_hash_exists() {
        let (pool, database_url) = create_test_db().await;

        let hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        sqlx::query("INSERT INTO Memories (id, content_hash, status) VALUES (99, ?1, 'PROCESSED')")
            .bind(hash)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO Memories (id, content_hash, status) VALUES (1, NULL, 'PENDING')")
            .execute(&pool)
            .await
            .unwrap();

        let is_duplicate = check_duplicate_in_db(hash, 1, &database_url).await.unwrap();
        assert!(
            is_duplicate,
            "must return true when a matching hash already exists"
        );

        let status: String = sqlx::query_scalar("SELECT status FROM Memories WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "DUPLICATE");
    }

    #[tokio::test]
    async fn duplicate_check_returns_false_when_hash_is_new() {
        let (pool, database_url) = create_test_db().await;

        sqlx::query("INSERT INTO Memories (id, content_hash, status) VALUES (1, NULL, 'PENDING')")
            .execute(&pool)
            .await
            .unwrap();

        let hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let is_duplicate = check_duplicate_in_db(hash, 1, &database_url).await.unwrap();
        assert!(!is_duplicate, "must return false when hash is unique");

        let status: String = sqlx::query_scalar("SELECT status FROM Memories WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "PENDING");
    }

    #[tokio::test]
    async fn resolve_existing_overlay_path_returns_none_for_missing_overlay() {
        let missing_overlay = Path::new("/tmp/processor-missing-overlay-test.png");

        let resolved = resolve_existing_overlay_path(Some(missing_overlay))
            .await
            .unwrap();

        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn resolve_existing_overlay_path_returns_path_for_existing_overlay() {
        let overlay_file = NamedTempFile::new().unwrap();

        let resolved = resolve_existing_overlay_path(Some(overlay_file.path()))
            .await
            .unwrap();

        assert_eq!(resolved.as_deref(), Some(overlay_file.path()));
    }

    #[tokio::test]
    async fn merge_staged_media_writes_output_when_overlay_is_missing() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().join("base.jpg");
        let missing_overlay_path = temp_dir.path().join("missing-overlay.png");
        let output_path = temp_dir.path().join("output.jpg");

        std::fs::write(&base_path, b"base-image-bytes").unwrap();

        merge_staged_media(
            &base_path,
            Some(&missing_overlay_path),
            &output_path,
            media::MediaEncodingOptions {
                video_profile: media::VideoOutputProfile::Mp4Compatible,
                image_format: media::ImageOutputFormat::Jpg,
                image_quality: media::ImageQuality::Full,
                hw_accel: media::HwAccelPreference::Disabled,
                overlay_strategy: media::OverlayStrategy::Upscale,
            },
        )
        .await
        .unwrap();

        assert!(output_path.exists());
    }

    #[test]
    fn detects_staging_paths() {
        assert!(is_staging_path(Path::new("/tmp/export/.staging/42.jpg")));
        assert!(!is_staging_path(Path::new(
            "/tmp/export/2026/02_February/42.jpg"
        )));
    }

    #[tokio::test]
    async fn cleanup_source_artifacts_removes_staging_files_even_when_keep_originals_true() {
        let temp_dir = tempdir().unwrap();
        let staging_dir = temp_dir.path().join(".staging");
        std::fs::create_dir_all(&staging_dir).unwrap();

        let staged_main = staging_dir.join("42.jpg");
        let staged_overlay = staging_dir.join("42.overlay.png");
        std::fs::write(&staged_main, b"main").unwrap();
        std::fs::write(&staged_overlay, b"overlay").unwrap();

        cleanup_source_artifacts(
            std::slice::from_ref(&staged_main),
            Some(&staged_overlay),
            None,
            true,
        )
        .await
        .unwrap();

        assert!(!staged_main.exists());
        assert!(!staged_overlay.exists());
        assert!(!staging_dir.exists());
    }

    #[test]
    fn builds_final_media_path_into_year_and_month_folder() {
        let export_root = Path::new("/tmp/export-root");

        let final_path = build_final_media_path(export_root, "2026-02-20", 42, "jpg").unwrap();

        assert_eq!(
            final_path,
            PathBuf::from("/tmp/export-root/2026/02_February/42.jpg")
        );
    }

    #[test]
    fn builds_final_media_path_from_rfc3339_datetime() {
        let export_root = Path::new("/tmp/export-root");

        let final_path =
            build_final_media_path(export_root, "2026-02-20T10:22:03Z", 7, "mp4").unwrap();

        assert_eq!(
            final_path,
            PathBuf::from("/tmp/export-root/2026/02_February/7.mp4")
        );
    }

    #[test]
    fn builds_thumbnail_path_in_thumbnail_root() {
        let thumbnail_root = Path::new("/tmp/export-root/.thumbnails");

        let thumbnail_path = build_thumbnail_path(thumbnail_root, 99);

        assert_eq!(
            thumbnail_path,
            PathBuf::from("/tmp/export-root/.thumbnails/99.webp")
        );
    }
}
