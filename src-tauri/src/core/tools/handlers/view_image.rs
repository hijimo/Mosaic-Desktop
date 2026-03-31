use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct ViewImageHandler;

pub const VIEW_IMAGE_TOOL_NAME: &str = "view_image";
const VIEW_IMAGE_UNSUPPORTED_MESSAGE: &str =
    "view_image is not allowed because the model does not support image inputs";

#[derive(Deserialize)]
struct ViewImageArgs {
    path: String,
}

/// Check if the model supports image input modality.
/// In the full implementation, this would check model_info.input_modalities.
/// For now, we accept a flag or default to true.
fn model_supports_images() -> bool {
    // TODO: wire to actual model_info.input_modalities check
    true
}

#[async_trait]
impl ToolHandler for ViewImageHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == VIEW_IMAGE_TOOL_NAME)
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin(VIEW_IMAGE_TOOL_NAME.to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Model capability check (matches source Codex behavior)
        if !model_supports_images() {
            return Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                VIEW_IMAGE_UNSUPPORTED_MESSAGE,
            ));
        }

        let params: ViewImageArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid view_image args: {e}"),
            )
        })?;

        let abs_path = std::path::PathBuf::from(&params.path);

        let metadata = tokio::fs::metadata(&abs_path).await.map_err(|e| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("unable to locate image at `{}`: {e}", abs_path.display()),
            )
        })?;

        if !metadata.is_file() {
            return Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("image path `{}` is not a file", abs_path.display()),
            ));
        }

        let data = tokio::fs::read(&abs_path).await.map_err(|e| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("failed to read image: {e}"),
            )
        })?;

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

        let mime = if params.path.ends_with(".png") {
            "image/png"
        } else if params.path.ends_with(".jpg") || params.path.ends_with(".jpeg") {
            "image/jpeg"
        } else if params.path.ends_with(".gif") {
            "image/gif"
        } else if params.path.ends_with(".webp") {
            "image/webp"
        } else if params.path.ends_with(".svg") {
            "image/svg+xml"
        } else {
            "application/octet-stream"
        };

        // Return structured content items matching source Codex FunctionCallOutputContentItem
        Ok(serde_json::json!({
            "content": [
                {
                    "type": "input_image",
                    "image_url": format!("data:{mime};base64,{b64}"),
                }
            ],
            "size_bytes": data.len(),
        }))
    }
}
