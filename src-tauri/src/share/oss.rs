use std::path::Path;

use aliyun_oss_rust_sdk::oss::OSS;
use aliyun_oss_rust_sdk::request::RequestBuilder;
use anyhow::{Context, Result};

use super::config::OssConfig;

pub async fn upload_file(
    config: &OssConfig,
    key: &str,
    source_path: &Path,
    content_type: Option<&str>,
) -> Result<()> {
    let bytes = tokio::fs::read(source_path)
        .await
        .with_context(|| format!("failed to read attachment: {}", source_path.display()))?;
    upload_bytes(config, key, bytes, content_type).await
}

pub async fn upload_bytes(
    config: &OssConfig,
    key: &str,
    bytes: Vec<u8>,
    content_type: Option<&str>,
) -> Result<()> {
    let access_key_id = config.access_key_id().to_string();
    let access_key_secret = config.access_key_secret().to_string();
    let bucket = config.bucket().to_string();
    let endpoint = config.endpoint();
    let key = key.to_string();
    let content_type = content_type.unwrap_or("application/octet-stream").to_string();

    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let oss = OSS::new(access_key_id, access_key_secret, endpoint, bucket);
        let builder = RequestBuilder::new()
            .with_expire(600)
            .with_content_type(content_type);

        oss.pub_object_from_buffer(&key, bytes.as_slice(), builder)
            .with_context(|| format!("failed to upload object to OSS: {key}"))?;

        Ok(())
    })
    .await
    .context("failed to join OSS upload task")??;

    Ok(())
}
