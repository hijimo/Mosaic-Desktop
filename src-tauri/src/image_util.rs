//! Shared image utilities: MIME detection, size-guarded base64 encoding, and compression.

use base64::Engine;
use std::path::Path;

/// Maximum raw file size allowed for image upload (20 MB).
const MAX_IMAGE_SIZE: usize = 20 * 1024 * 1024;

/// Images smaller than this threshold skip compression (50 KB).
const COMPRESS_THRESHOLD: usize = 50 * 1024;

/// Target quality for JPEG re-encoding (1–100).
const JPEG_QUALITY: u8 = 80;

/// Infer MIME type from file extension.
pub fn mime_for_ext(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

/// Read an image from disk, optionally compress it, and return `data:{mime};base64,…` string.
///
/// - Files > 20 MB are rejected.
/// - Files > 50 KB (non-SVG, non-PNG) are re-encoded as JPEG at quality 80 to reduce size.
/// - PNG files are sent as-is to preserve alpha channel.
/// - Files ≤ 50 KB are sent as-is.
pub async fn load_image_as_data_url(path: &Path) -> Result<String, String> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| format!("unable to read image `{}`: {e}", path.display()))?;

    if data.len() > MAX_IMAGE_SIZE {
        return Err(format!(
            "image `{}` is too large ({:.1} MB, max {} MB)",
            path.display(),
            data.len() as f64 / 1_048_576.0,
            MAX_IMAGE_SIZE / 1_048_576
        ));
    }

    let original_mime = mime_for_ext(path);

    // SVGs, small files, and PNGs (preserve alpha): send as-is
    if original_mime == "image/svg+xml"
        || original_mime == "image/png"
        || data.len() <= COMPRESS_THRESHOLD
    {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        return Ok(format!("data:{original_mime};base64,{b64}"));
    }

    // Attempt compression via re-encoding to JPEG
    match compress_to_jpeg(&data, JPEG_QUALITY) {
        Ok(compressed) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&compressed);
            Ok(format!("data:image/jpeg;base64,{b64}"))
        }
        Err(_) => {
            // Fallback: send original bytes
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            Ok(format!("data:{original_mime};base64,{b64}"))
        }
    }
}

/// Decode image bytes and re-encode as JPEG at the given quality.
fn compress_to_jpeg(data: &[u8], quality: u8) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(data).map_err(|e| format!("decode failed: {e}"))?;

    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode_image(&img)
        .map_err(|e| format!("jpeg encode failed: {e}"))?;

    if out.len() < data.len() {
        Ok(out)
    } else {
        Err("compressed size not smaller".into())
    }
}
