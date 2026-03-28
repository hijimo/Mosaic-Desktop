use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use aliyun_oss_rust_sdk::oss::OSS;
use aliyun_oss_rust_sdk::request::RequestBuilder;
use tauri_app_lib::share::config::OssConfig;
use tauri_app_lib::share::oss::{upload_bytes, upload_file};
use tauri_app_lib::share::share_message;
use tauri_app_lib::share::types::ShareMessageRequest;

fn load_env() {
    let env_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    let _ = dotenvy::from_path_override(&env_path);
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("share-oss-upload.txt")
}

fn unique_key(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    format!("{prefix}integration-tests/share-oss-{nanos}.txt")
}

fn sdk_client(config: &OssConfig) -> OSS {
    OSS::new(
        config.access_key_id().to_string(),
        config.access_key_secret().to_string(),
        config.endpoint(),
        config.bucket().to_string(),
    )
}

#[tokio::test]
#[ignore]
async fn uploads_fixture_file_to_oss() -> Result<()> {
    load_env();

    let config = OssConfig::from_env()?;
    let source_path = fixture_path();
    let object_key = unique_key(&config.normalized_prefix());

    upload_file(&config, &object_key, &source_path, Some("text/plain; charset=utf-8"))
        .await
        .with_context(|| format!("failed to upload fixture: {}", source_path.display()))?;

    let client = sdk_client(&config);
    let metadata_key = object_key.clone();
    let metadata = tokio::task::spawn_blocking(move || {
        client
            .get_object_metadata(&metadata_key, RequestBuilder::new())
            .with_context(|| format!("failed to fetch metadata for test object: {metadata_key}"))
    })
    .await
    .context("failed to join OSS metadata task")??;
    assert_eq!(metadata.content_disposition().as_deref(), Some("attachment"));

    let client = sdk_client(&config);
    let delete_key = object_key.clone();
    tokio::task::spawn_blocking(move || {
        client
            .delete_object(&delete_key, RequestBuilder::new())
            .with_context(|| format!("failed to delete test object: {delete_key}"))
    })
    .await
    .context("failed to join OSS delete task")??;

    Ok(())
}

#[tokio::test]
#[ignore]
async fn uploads_html_as_inline_preview() -> Result<()> {
    load_env();

    let config = OssConfig::from_env()?;
    let object_key = format!("{}integration-tests/share-preview-inline.html", config.normalized_prefix());

    upload_bytes(
        &config,
        &object_key,
        b"<html><body>preview</body></html>".to_vec(),
        Some("text/html; charset=utf-8"),
    )
    .await
    .with_context(|| format!("failed to upload preview object: {object_key}"))?;

    let client = sdk_client(&config);
    let metadata_key = object_key.clone();
    let metadata = tokio::task::spawn_blocking(move || {
        client
            .get_object_metadata(&metadata_key, RequestBuilder::new())
            .with_context(|| format!("failed to fetch metadata for preview object: {metadata_key}"))
    })
    .await
    .context("failed to join OSS metadata task")??;
    assert_eq!(metadata.content_disposition().as_deref(), Some("inline"));

    let client = sdk_client(&config);
    let delete_key = object_key.clone();
    tokio::task::spawn_blocking(move || {
        client
            .delete_object(&delete_key, RequestBuilder::new())
            .with_context(|| format!("failed to delete preview object: {delete_key}"))
    })
    .await
    .context("failed to join OSS delete task")??;

    Ok(())
}

#[tokio::test]
#[ignore]
async fn share_message_uploads_index_html_as_inline() -> Result<()> {
    load_env();

    let config = OssConfig::from_env()?;
    let response = share_message(ShareMessageRequest {
        turn_id: "turn-inline-check".into(),
        title: "inline-check".into(),
        generated_at: "2026-03-28T00:00:00Z".into(),
        user_text: "hello".into(),
        answer_markdown: "<b>world</b>".into(),
        attachments: Vec::new(),
    })
    .await
    .context("failed to share message")?;

    let base = format!("{}/", config.public_base_url());
    let object_key = response
        .url
        .strip_prefix(&base)
        .map(str::to_string)
        .with_context(|| format!("share url does not start with configured host: {}", response.url))?;

    let client = sdk_client(&config);
    let metadata_key = object_key.clone();
    let metadata = tokio::task::spawn_blocking(move || {
        client
            .get_object_metadata(&metadata_key, RequestBuilder::new())
            .with_context(|| format!("failed to fetch metadata for shared page: {metadata_key}"))
    })
    .await
    .context("failed to join OSS metadata task")??;
    assert_eq!(metadata.content_disposition().as_deref(), Some("inline"));

    let client = sdk_client(&config);
    let delete_index_key = object_key.clone();
    tokio::task::spawn_blocking(move || {
        client
            .delete_object(&delete_index_key, RequestBuilder::new())
            .with_context(|| format!("failed to delete shared page: {delete_index_key}"))
    })
    .await
    .context("failed to join OSS delete task")??;

    Ok(())
}
