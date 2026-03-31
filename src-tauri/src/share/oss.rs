use std::path::Path;

use aliyun_oss_rust_sdk::oss::OSS;
use aliyun_oss_rust_sdk::request::RequestBuilder;
use aliyun_oss_rust_sdk::url::UrlApi;
use anyhow::{Context, Result};
use reqwest::header::{HeaderValue, CONTENT_DISPOSITION, CONTENT_TYPE};
use tracing::info;

use super::config::OssConfig;

const OSS_REQUEST_EXPIRE_SECONDS: i64 = 600;

pub async fn upload_file(
    config: &OssConfig,
    key: &str,
    source_path: &Path,
    content_type: Option<&str>,
) -> Result<()> {
    let client = build_client(config);
    let object_key = normalize_object_key(key);
    let file_path = source_path.to_string_lossy().into_owned();
    let builder = build_request(content_type);
    let content_type = content_type.map(str::to_owned);
    let content_disposition = content_disposition_for(content_type.as_deref()).to_string();
    let signed_url = client.sign_upload_url(&object_key, &builder);
    let display_path = source_path.display().to_string();

    info!("uploading file to OSS: key={object_key}, path={display_path}");

    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&file_path)
            .with_context(|| format!("failed to read file for OSS upload: {file_path}"))?;

        upload_with_headers(
            signed_url,
            bytes,
            object_key.clone(),
            content_type.clone(),
            content_disposition.clone(),
        )
        .with_context(|| format!("failed to upload file to OSS: {object_key}"))
    })
    .await
    .context("failed to join OSS file upload task")??;

    Ok(())
}

pub async fn upload_bytes(
    config: &OssConfig,
    key: &str,
    bytes: Vec<u8>,
    content_type: Option<&str>,
) -> Result<()> {
    let client = build_client(config);
    let object_key = normalize_object_key(key);
    let builder = build_request(content_type);
    let content_type = content_type.map(str::to_owned);
    let content_disposition = content_disposition_for(content_type.as_deref()).to_string();
    let signed_url = client.sign_upload_url(&object_key, &builder);

    info!(
        "uploading in-memory payload to OSS: key={object_key}, bytes={}",
        bytes.len()
    );

    tauri::async_runtime::spawn_blocking(move || {
        upload_with_headers(
            signed_url,
            bytes,
            object_key.clone(),
            content_type.clone(),
            content_disposition.clone(),
        )
        .with_context(|| format!("failed to upload bytes to OSS: {object_key}"))
    })
    .await
    .context("failed to join OSS bytes upload task")??;

    Ok(())
}

fn build_client(config: &OssConfig) -> OSS {
    OSS::new(
        config.access_key_id().to_string(),
        config.access_key_secret().to_string(),
        config.endpoint(),
        config.bucket().to_string(),
    )
}

fn build_request(content_type: Option<&str>) -> RequestBuilder {
    let builder = RequestBuilder::new().with_expire(OSS_REQUEST_EXPIRE_SECONDS);
    match content_type {
        Some(value) if !value.trim().is_empty() => builder.with_content_type(value),
        _ => builder,
    }
}

fn content_disposition_for(content_type: Option<&str>) -> &'static str {
    let Some(content_type) = content_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return "attachment";
    };

    let mime = content_type
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or(content_type)
        .to_ascii_lowercase();

    if mime == "text/html" || mime.starts_with("image/") {
        "inline"
    } else {
        "attachment"
    }
}

fn upload_with_headers(
    url: String,
    bytes: Vec<u8>,
    object_key: String,
    content_type: Option<String>,
    content_disposition: String,
) -> Result<()> {
    let mut request = reqwest::blocking::Client::new().put(&url).header(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .with_context(|| format!("invalid content-disposition: {content_disposition}"))?,
    );

    if let Some(content_type) = content_type {
        request = request.header(
            CONTENT_TYPE,
            HeaderValue::from_str(&content_type)
                .with_context(|| format!("invalid content-type: {content_type}"))?,
        );
    }

    let response = request
        .body(bytes)
        .send()
        .with_context(|| format!("failed to send OSS upload request: {object_key}"))?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    let body = response
        .text()
        .with_context(|| format!("failed to read OSS error response: {object_key}"))?;
    anyhow::bail!("OSS upload failed for {object_key}: status={status}; body={body}");
}

fn normalize_object_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_request, content_disposition_for, normalize_object_key, OSS_REQUEST_EXPIRE_SECONDS,
    };

    #[test]
    fn normalize_object_key_adds_leading_slash() {
        assert_eq!(normalize_object_key("share/demo.txt"), "/share/demo.txt");
    }

    #[test]
    fn normalize_object_key_preserves_existing_slash() {
        assert_eq!(normalize_object_key("/share/demo.txt"), "/share/demo.txt");
    }

    #[test]
    fn build_request_sets_content_type_when_present() {
        let builder = build_request(Some("text/plain"));

        assert_eq!(builder.expire, OSS_REQUEST_EXPIRE_SECONDS);
        assert_eq!(builder.content_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn html_content_is_inline() {
        assert_eq!(
            content_disposition_for(Some("text/html; charset=utf-8")),
            "inline"
        );
    }

    #[test]
    fn image_content_is_inline() {
        assert_eq!(content_disposition_for(Some("image/png")), "inline");
    }

    #[test]
    fn other_content_defaults_to_attachment() {
        assert_eq!(
            content_disposition_for(Some("application/pdf")),
            "attachment"
        );
        assert_eq!(content_disposition_for(None), "attachment");
    }
}
