pub mod config;
pub mod oss;
pub mod render;
pub mod types;

use std::path::Path;

use anyhow::{Context, Result};
use uuid::Uuid;

use self::config::OssConfig;
use self::types::{
    ShareAttachment, ShareAttachmentInput, ShareMessageRequest, ShareMessageResponse, SharePage,
};

pub async fn share_message(payload: ShareMessageRequest) -> Result<ShareMessageResponse> {
    let config = OssConfig::from_env()?;
    let share_id = Uuid::now_v7().to_string();

    let attachments = upload_attachments(&config, &share_id, &payload.attachments).await?;
    let page = SharePage {
        title: payload.title,
        generated_at: payload.generated_at,
        user_html: render_markdownish_html(&payload.user_text),
        answer_html: render_markdownish_html(&payload.answer_markdown),
        attachments,
    };

    let html = render::render_share_html(&page);
    let index_key = build_object_key(&config, &share_id, "index.html");
    oss::upload_bytes(
        &config,
        &index_key,
        html.into_bytes(),
        Some("text/html; charset=utf-8"),
    )
    .await?;

    Ok(ShareMessageResponse {
        url: config.public_url_for_key(&index_key),
    })
}

async fn upload_attachments(
    config: &OssConfig,
    share_id: &str,
    attachments: &[ShareAttachmentInput],
) -> Result<Vec<ShareAttachment>> {
    let mut uploaded = Vec::with_capacity(attachments.len());

    for (index, attachment) in attachments.iter().enumerate() {
        let source_path = Path::new(&attachment.source_path);
        let safe_name = sanitize_file_name(&attachment.display_name);
        let object_key =
            build_object_key(config, share_id, &format!("assets/{index:02}-{safe_name}"));

        oss::upload_file(
            config,
            &object_key,
            source_path,
            attachment.content_type.as_deref(),
        )
        .await
        .with_context(|| format!("failed to upload attachment: {}", attachment.display_name))?;

        uploaded.push(ShareAttachment {
            kind: attachment.kind.clone(),
            display_name: attachment.display_name.clone(),
            url: config.public_url_for_key(&object_key),
            content_type: attachment.content_type.clone(),
        });
    }

    Ok(uploaded)
}

fn build_object_key(config: &OssConfig, share_id: &str, file_name: &str) -> String {
    format!(
        "{}{}/{}",
        config.normalized_prefix(),
        share_id,
        file_name.trim_start_matches('/')
    )
}

fn sanitize_file_name(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            sanitized.push(ch);
        } else {
            sanitized.push('-');
        }
    }

    sanitized
        .trim_matches('-')
        .to_string()
        .if_empty("attachment")
}

fn render_markdownish_html(markdown: &str) -> String {
    let mut html = String::new();
    let mut paragraph_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_language: Option<String> = None;

    for line in markdown.lines() {
        if let Some(language) = line.trim_start().strip_prefix("```") {
            if code_language.is_some() {
                flush_code_block(&mut html, &mut code_lines, code_language.take());
            } else {
                flush_paragraph(&mut html, &mut paragraph_lines);
                code_language = Some(language.trim().to_string());
            }
            continue;
        }

        if code_language.is_some() {
            code_lines.push(line.to_string());
            continue;
        }

        if line.trim().is_empty() {
            flush_paragraph(&mut html, &mut paragraph_lines);
        } else {
            paragraph_lines.push(line.to_string());
        }
    }

    flush_paragraph(&mut html, &mut paragraph_lines);
    flush_code_block(&mut html, &mut code_lines, code_language.take());

    if html.is_empty() {
        "<p></p>".to_string()
    } else {
        html
    }
}

fn flush_paragraph(html: &mut String, paragraph_lines: &mut Vec<String>) {
    if paragraph_lines.is_empty() {
        return;
    }

    let content = paragraph_lines.join("\n");
    let trimmed = content.trim();
    if trimmed.is_empty() {
        paragraph_lines.clear();
        return;
    }

    if let Some(text) = trimmed.strip_prefix("### ") {
        html.push_str(&format!("<h3>{}</h3>", escape_html(text.trim())));
    } else if let Some(text) = trimmed.strip_prefix("## ") {
        html.push_str(&format!("<h2>{}</h2>", escape_html(text.trim())));
    } else if let Some(text) = trimmed.strip_prefix("# ") {
        html.push_str(&format!("<h1>{}</h1>", escape_html(text.trim())));
    } else {
        html.push_str("<p>");
        html.push_str(&escape_html(trimmed).replace('\n', "<br />"));
        html.push_str("</p>");
    }

    paragraph_lines.clear();
}

fn flush_code_block(
    html: &mut String,
    code_lines: &mut Vec<String>,
    code_language: Option<String>,
) {
    let Some(language) = code_language else {
        code_lines.clear();
        return;
    };

    let class_name = sanitize_code_language(&language);
    let language_attr = if class_name.is_empty() {
        String::new()
    } else {
        format!(r#" class="language-{}""#, class_name)
    };

    html.push_str("<pre><code");
    html.push_str(&language_attr);
    html.push('>');
    html.push_str(&escape_html(&code_lines.join("\n")));
    html.push_str("</code></pre>");
    code_lines.clear();
}

fn sanitize_code_language(language: &str) -> String {
    language
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect()
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

trait StringExt {
    fn if_empty(self, fallback: &str) -> String;
}

impl StringExt for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{render_markdownish_html, sanitize_file_name};

    #[test]
    fn markdownish_renderer_supports_headings_and_code() {
        let html = render_markdownish_html("# 标题\n\n正文\n\n```ts\nconsole.log(1)\n```");

        assert!(html.contains("<h1>标题</h1>"));
        assert!(html.contains("<p>正文</p>"));
        assert!(html.contains("language-ts"));
        assert!(html.contains("console.log(1)"));
    }

    #[test]
    fn sanitize_file_name_keeps_extension() {
        assert_eq!(sanitize_file_name("图表 / demo?.png"), "demo-.png");
    }
}
