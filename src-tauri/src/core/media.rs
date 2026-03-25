use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Video,
}

#[derive(Debug)]
pub enum MediaError {
    Io(std::io::Error),
    Join(tokio::task::JoinError),
    UnsupportedMediaType(PathBuf),
    MissingOverlay(PathBuf),
    InvalidMetadata(String),
    FfmpegFailed { status: Option<i32>, stderr: String },
}

impl Display for MediaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "media processing I/O failed: {error}"),
            Self::Join(error) => write!(f, "media processing thread failed: {error}"),
            Self::UnsupportedMediaType(path) => {
                write!(
                    f,
                    "unsupported media type for '{}': expected JPEG/PNG or MP4/MOV",
                    path.display()
                )
            }
            Self::MissingOverlay(path) => {
                write!(f, "overlay file does not exist at '{}'", path.display())
            }
            Self::InvalidMetadata(reason) => write!(f, "invalid media metadata: {reason}"),
            Self::FfmpegFailed { status, stderr } => {
                write!(
                    f,
                    "ffmpeg exited with status {:?}: {}",
                    status,
                    stderr.trim()
                )
            }
        }
    }
}

impl std::error::Error for MediaError {}

impl From<std::io::Error> for MediaError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tokio::task::JoinError> for MediaError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value)
    }
}

pub async fn merge_media_with_optional_overlay(
    base_media_path: &Path,
    overlay_path: Option<&Path>,
    output_path: &Path,
) -> Result<(), MediaError> {
    let base_media_path = base_media_path.to_path_buf();
    let overlay_path = overlay_path.map(Path::to_path_buf);
    let output_path = output_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match overlay_path {
            Some(overlay_path) => {
                if !overlay_path.exists() {
                    return Err(MediaError::MissingOverlay(overlay_path));
                }

                let media_kind = media_kind_from_path(&base_media_path)
                    .ok_or_else(|| MediaError::UnsupportedMediaType(base_media_path.clone()))?;

                let writes_in_place = paths_point_to_same_target(&base_media_path, &output_path);
                let ffmpeg_output_path = if writes_in_place {
                    temporary_output_path_with_suffix(&output_path, "overlay.tmp")?
                } else {
                    output_path.clone()
                };

                let args = build_ffmpeg_overlay_args(
                    &base_media_path,
                    &overlay_path,
                    &ffmpeg_output_path,
                    media_kind,
                );

                run_ffmpeg(args)?;

                if writes_in_place {
                    std::fs::rename(&ffmpeg_output_path, &output_path)?;
                }

                Ok(())
            }
            None => {
                if paths_point_to_same_target(&base_media_path, &output_path) {
                    return Ok(());
                }

                std::fs::copy(&base_media_path, &output_path)?;
                Ok(())
            }
        }
    })
    .await??;

    Ok(())
}

pub async fn write_metadata_with_ffmpeg(
    media_path: &Path,
    date_taken: &str,
    location: Option<&str>,
) -> Result<(), MediaError> {
    let media_path = media_path.to_path_buf();
    let date_taken = normalize_datetime_for_ffmpeg(date_taken)?;
    let coordinates = location.and_then(parse_coordinates);

    tokio::task::spawn_blocking(move || {
        let media_kind = media_kind_from_path(&media_path)
            .ok_or_else(|| MediaError::UnsupportedMediaType(media_path.clone()))?;

        let temp_output_path = temporary_output_path(&media_path)?;
        let args = build_ffmpeg_metadata_args(
            &media_path,
            &temp_output_path,
            &date_taken,
            coordinates,
            media_kind,
        );

        run_ffmpeg(args)?;
        std::fs::rename(&temp_output_path, &media_path)?;
        Ok::<(), MediaError>(())
    })
    .await??;

    Ok(())
}

pub async fn cleanup_intermediate_files(
    raw_media_path: &Path,
    overlay_path: Option<&Path>,
    final_media_path: &Path,
) -> Result<(), MediaError> {
    let raw_media_path = raw_media_path.to_path_buf();
    let overlay_path = overlay_path.map(Path::to_path_buf);
    let final_media_path = final_media_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        if raw_media_path != final_media_path {
            remove_file_if_exists(&raw_media_path)?;
        }

        if let Some(overlay_path) = overlay_path {
            if overlay_path != final_media_path {
                remove_file_if_exists(&overlay_path)?;
            }
        }

        Ok::<(), MediaError>(())
    })
    .await??;

    Ok(())
}

fn run_ffmpeg(args: Vec<String>) -> Result<(), MediaError> {
    eprintln!("[processor-debug] ffmpeg args={}", args.join(" "));

    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(MediaError::Io)?;

    if output.status.success() {
        return Ok(());
    }

    eprintln!(
        "[processor-debug] ffmpeg failure status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim()
    );

    Err(MediaError::FfmpegFailed {
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn media_kind_from_path(path: &Path) -> Option<MediaKind> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();

    match extension.as_str() {
        "jpg" | "jpeg" | "png" => Some(MediaKind::Image),
        "mp4" | "mov" => Some(MediaKind::Video),
        _ => None,
    }
}

fn build_ffmpeg_overlay_args(
    base_media_path: &Path,
    overlay_path: &Path,
    output_path: &Path,
    media_kind: MediaKind,
) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        base_media_path.to_string_lossy().to_string(),
        "-i".to_string(),
        overlay_path.to_string_lossy().to_string(),
    ];

    match media_kind {
        MediaKind::Image => {
            args.push("-filter_complex".to_string());
            args.push("[0:v][1:v]overlay=0:0:format=auto".to_string());
            args.push("-frames:v".to_string());
            args.push("1".to_string());
        }
        MediaKind::Video => {
            // Scale the overlay to match the base video dimensions before compositing,
            // so the overlay fills the full frame regardless of the PNG's native size.
            args.push("-filter_complex".to_string());
            args.push("[1:v][0:v]scale2ref[ov][base];[base][ov]overlay=0:0:format=auto".to_string());
            args.push("-map".to_string());
            args.push("0:a?".to_string());
            args.push("-c:a".to_string());
            args.push("copy".to_string());
            args.push("-c:v".to_string());
            args.push("libx264".to_string());
            args.push("-crf".to_string());
            args.push("18".to_string());
            args.push("-preset".to_string());
            args.push("veryfast".to_string());
        }
    }

    args.push(output_path.to_string_lossy().to_string());
    args
}

fn build_ffmpeg_metadata_args(
    media_path: &Path,
    output_path: &Path,
    date_taken: &str,
    coordinates: Option<(f64, f64)>,
    media_kind: MediaKind,
) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        media_path.to_string_lossy().to_string(),
        "-metadata".to_string(),
        format!("DateTimeOriginal={date_taken}"),
        "-metadata".to_string(),
        format!("DateTimeDigitized={date_taken}"),
        "-metadata".to_string(),
        format!("creation_time={date_taken}"),
    ];

    if let Some((latitude, longitude)) = coordinates {
        args.push("-metadata".to_string());
        args.push(format!("GPSLatitude={latitude}"));
        args.push("-metadata".to_string());
        args.push(format!("GPSLongitude={longitude}"));
        args.push("-metadata".to_string());
        args.push(format!(
            "location={}{}{}{}",
            if latitude >= 0.0 { "+" } else { "" },
            latitude,
            if longitude >= 0.0 { "+" } else { "" },
            longitude
        ));
    }

    match media_kind {
        MediaKind::Image => {
            args.push("-frames:v".to_string());
            args.push("1".to_string());
        }
        MediaKind::Video => {
            args.push("-map".to_string());
            args.push("0:v".to_string());
            args.push("-map".to_string());
            args.push("0:a?".to_string());
            args.push("-dn".to_string());
            args.push("-c".to_string());
            args.push("copy".to_string());
        }
    }

    args.push(output_path.to_string_lossy().to_string());
    args
}

fn normalize_datetime_for_ffmpeg(value: &str) -> Result<String, MediaError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MediaError::InvalidMetadata("date is empty".to_string()));
    }

    if trimmed.len() >= 19 && trimmed.as_bytes().get(10) == Some(&b'T') {
        let date = &trimmed[..10];
        let time = &trimmed[11..19];
        return Ok(format!("{} {}", date.replace('-', ":"), time));
    }

    if trimmed.len() >= 19
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(7) == Some(&b'-')
        && trimmed.as_bytes().get(10) == Some(&b' ')
    {
        let date = &trimmed[..10];
        let time = &trimmed[11..19];
        return Ok(format!("{} {}", date.replace('-', ":"), time));
    }

    Ok(trimmed.to_string())
}

fn parse_coordinates(value: &str) -> Option<(f64, f64)> {
    let mut parts = value.split(',').map(str::trim);
    let latitude = parts.next()?.parse::<f64>().ok()?;
    let longitude = parts.next()?.parse::<f64>().ok()?;
    Some((latitude, longitude))
}

fn temporary_output_path(media_path: &Path) -> Result<PathBuf, MediaError> {
    temporary_output_path_with_suffix(media_path, "metadata.tmp")
}

fn temporary_output_path_with_suffix(media_path: &Path, suffix: &str) -> Result<PathBuf, MediaError> {
    let stem = media_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            MediaError::InvalidMetadata("media path does not contain a valid file stem".to_string())
        })?;

    let extension = media_path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            MediaError::InvalidMetadata("media path does not contain a valid extension".to_string())
        })?;

    Ok(media_path.with_file_name(format!("{stem}.{suffix}.{extension}")))
}

fn paths_point_to_same_target(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    let left_canonical = std::fs::canonicalize(left).ok();
    let right_canonical = std::fs::canonicalize(right).ok();

    match (left_canonical, right_canonical) {
        (Some(left_canonical), Some(right_canonical)) => left_canonical == right_canonical,
        _ => false,
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), MediaError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use tempfile::tempdir;

    use super::{
        build_ffmpeg_metadata_args, build_ffmpeg_overlay_args, cleanup_intermediate_files,
        media_kind_from_path, normalize_datetime_for_ffmpeg, parse_coordinates, MediaKind,
    };

    #[test]
    fn detects_supported_media_kinds() {
        assert_eq!(
            media_kind_from_path(Path::new("memory.jpg")),
            Some(MediaKind::Image)
        );
        assert_eq!(
            media_kind_from_path(Path::new("memory.JPEG")),
            Some(MediaKind::Image)
        );
        assert_eq!(
            media_kind_from_path(Path::new("memory.mp4")),
            Some(MediaKind::Video)
        );
        assert_eq!(
            media_kind_from_path(Path::new("memory.mov")),
            Some(MediaKind::Video)
        );
        assert_eq!(media_kind_from_path(Path::new("memory.webm")), None);
    }

    #[test]
    fn builds_overlay_arguments_for_images() {
        let args = build_ffmpeg_overlay_args(
            Path::new("base.jpg"),
            Path::new("overlay.png"),
            Path::new("output.jpg"),
            MediaKind::Image,
        );

        assert!(args.contains(&"-frames:v".to_string()));
        assert!(!args.contains(&"libx264".to_string()));
    }

    #[test]
    fn builds_overlay_arguments_for_videos() {
        let args = build_ffmpeg_overlay_args(
            Path::new("base.mp4"),
            Path::new("overlay.png"),
            Path::new("output.mp4"),
            MediaKind::Video,
        );

        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"0:a?".to_string()));
        assert!(!args.contains(&"-frames:v".to_string()));
    }

    #[test]
    fn normalizes_rfc3339_datetime_for_ffmpeg() {
        let formatted = normalize_datetime_for_ffmpeg("2024-03-01T12:13:14Z")
            .expect("datetime should be normalized");
        assert_eq!(formatted, "2024:03:01 12:13:14");
    }

    #[test]
    fn parses_coordinates_from_location_string() {
        let coordinates = parse_coordinates("48.8566,2.3522").expect("coordinates should parse");
        assert_eq!(coordinates.0, 48.8566);
        assert_eq!(coordinates.1, 2.3522);
    }

    #[test]
    fn builds_metadata_arguments_with_gps_tags() {
        let args = build_ffmpeg_metadata_args(
            Path::new("output.jpg"),
            Path::new("output.jpg.metadata.tmp"),
            "2024:03:01 12:13:14",
            Some((12.34, -56.78)),
            MediaKind::Image,
        );

        assert!(args
            .iter()
            .any(|arg| arg.contains("DateTimeOriginal=2024:03:01 12:13:14")));
        assert!(args.iter().any(|arg| arg.contains("GPSLatitude=12.34")));
        assert!(args.iter().any(|arg| arg.contains("GPSLongitude=-56.78")));
    }

    #[tokio::test]
    async fn cleanup_deletes_raw_and_overlay_keeps_final() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let raw_path = temp_dir.path().join("raw.jpg");
        let overlay_path = temp_dir.path().join("overlay.png");
        let final_path = temp_dir.path().join("final.jpg");

        std::fs::write(&raw_path, b"raw").expect("raw file should be created");
        std::fs::write(&overlay_path, b"overlay").expect("overlay file should be created");
        std::fs::write(&final_path, b"final").expect("final file should be created");

        cleanup_intermediate_files(&raw_path, Some(&overlay_path), &final_path)
            .await
            .expect("cleanup should succeed");

        assert!(!raw_path.exists());
        assert!(!overlay_path.exists());
        assert!(final_path.exists());
    }

    #[tokio::test]
    async fn cleanup_does_not_delete_final_when_paths_match() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let final_path = temp_dir.path().join("final.jpg");

        std::fs::write(&final_path, b"final").expect("final file should be created");

        cleanup_intermediate_files(&final_path, Some(&final_path), &final_path)
            .await
            .expect("cleanup should succeed when paths match");

        assert!(final_path.exists());
    }
}
