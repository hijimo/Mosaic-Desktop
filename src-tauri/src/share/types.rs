#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShareAttachmentKind {
    Image,
    File,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShareAttachmentInput {
    pub kind: ShareAttachmentKind,
    pub source_path: String,
    pub display_name: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShareMessageRequest {
    pub turn_id: String,
    pub title: String,
    pub generated_at: String,
    pub user_text: String,
    pub answer_markdown: String,
    pub attachments: Vec<ShareAttachmentInput>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ShareMessageResponse {
    pub url: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShareAttachment {
    pub kind: ShareAttachmentKind,
    pub display_name: String,
    pub url: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharePage {
    pub title: String,
    pub generated_at: String,
    pub user_html: String,
    pub answer_html: String,
    pub attachments: Vec<ShareAttachment>,
}
