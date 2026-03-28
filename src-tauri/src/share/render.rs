use super::types::{ShareAttachment, ShareAttachmentKind, SharePage};

pub fn render_share_html(page: &SharePage) -> String {
    let attachment_section = if page.attachments.is_empty() {
        String::new()
    } else {
        format!(
            r#"<section class="card">
  <h2>附件</h2>
  <div class="attachments">{}</div>
</section>"#,
            page.attachments
                .iter()
                .map(render_attachment)
                .collect::<Vec<_>>()
                .join("")
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
    <style>
      :root {{
        color-scheme: light;
        --bg: #f3f5f8;
        --panel: rgba(255, 255, 255, 0.92);
        --text: #18212b;
        --muted: #66758a;
        --line: rgba(130, 145, 166, 0.24);
        --accent: #1463ff;
        --shadow: 0 24px 60px rgba(16, 24, 40, 0.08);
        --code-bg: #0f172a;
        --code-text: #e2e8f0;
      }}

      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        background:
          radial-gradient(circle at top, rgba(20, 99, 255, 0.12), transparent 38%),
          linear-gradient(180deg, #fbfcfe 0%, var(--bg) 100%);
        color: var(--text);
      }}
      main {{
        max-width: 960px;
        margin: 0 auto;
        padding: 32px 20px 56px;
      }}
      header {{
        margin-bottom: 20px;
      }}
      h1 {{
        margin: 0 0 8px;
        font-size: 32px;
        line-height: 1.2;
      }}
      .meta {{
        color: var(--muted);
        font-size: 14px;
      }}
      .card {{
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: 24px;
        box-shadow: var(--shadow);
        padding: 24px;
        backdrop-filter: blur(12px);
        margin-bottom: 16px;
      }}
      .card h2 {{
        margin: 0 0 16px;
        font-size: 20px;
      }}
      .content {{
        font-size: 15px;
        line-height: 1.75;
      }}
      .content h1, .content h2, .content h3 {{
        margin: 24px 0 12px;
        line-height: 1.35;
      }}
      .content p {{
        margin: 0 0 14px;
        white-space: normal;
      }}
      .content pre {{
        margin: 18px 0;
        padding: 18px;
        overflow: auto;
        border-radius: 18px;
        background: var(--code-bg);
        color: var(--code-text);
      }}
      .content code {{
        font-family: "SFMono-Regular", "SF Mono", Consolas, monospace;
      }}
      .attachments {{
        display: grid;
        gap: 16px;
      }}
      .attachment {{
        display: block;
        border: 1px solid var(--line);
        border-radius: 18px;
        padding: 14px;
        text-decoration: none;
        color: inherit;
        background: rgba(255, 255, 255, 0.72);
      }}
      .attachment img {{
        display: block;
        width: 100%;
        max-height: 460px;
        object-fit: contain;
        border-radius: 12px;
        background: #eef2f7;
      }}
      .attachment figcaption {{
        margin-top: 10px;
        color: var(--muted);
        font-size: 14px;
      }}
      .file-link {{
        color: var(--accent);
        font-weight: 600;
      }}
      @media (max-width: 720px) {{
        main {{
          padding: 20px 14px 36px;
        }}
        .card {{
          padding: 18px;
          border-radius: 20px;
        }}
        h1 {{
          font-size: 26px;
        }}
      }}
    </style>
  </head>
  <body>
    <main>
      <header>
        <h1>{title}</h1>
        <div class="meta">生成时间：{generated_at}</div>
      </header>
      <section class="card">
        <h2>用户问题</h2>
        <div class="content">{user_html}</div>
      </section>
      <section class="card">
        <h2>助手回答</h2>
        <div class="content">{answer_html}</div>
      </section>
      {attachment_section}
    </main>
  </body>
</html>"#,
        title = escape_html(&page.title),
        generated_at = escape_html(&page.generated_at),
        user_html = page.user_html,
        answer_html = page.answer_html,
        attachment_section = attachment_section,
    )
}

fn render_attachment(attachment: &ShareAttachment) -> String {
    match attachment.kind {
        ShareAttachmentKind::Image => format!(
            r#"<figure class="attachment">
  <img src="{url}" alt="{name}" />
  <figcaption>{name}</figcaption>
</figure>"#,
            url = escape_html(&attachment.url),
            name = escape_html(&attachment.display_name),
        ),
        ShareAttachmentKind::File => format!(
            r#"<a class="attachment file-link" href="{url}" target="_blank" rel="noreferrer">{name}</a>"#,
            url = escape_html(&attachment.url),
            name = escape_html(&attachment.display_name),
        ),
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::render_share_html;
    use crate::share::types::{ShareAttachment, ShareAttachmentKind, SharePage};

    #[test]
    fn omits_attachment_section_when_empty() {
        let html = render_share_html(&SharePage {
            title: "标题".into(),
            generated_at: "2026-03-28T12:00:00Z".into(),
            user_html: "<p>用户</p>".into(),
            answer_html: "<p>回答</p>".into(),
            attachments: Vec::new(),
        });

        assert!(!html.contains("<h2>附件</h2>"));
    }

    #[test]
    fn renders_image_attachment() {
        let html = render_share_html(&SharePage {
            title: "标题".into(),
            generated_at: "2026-03-28T12:00:00Z".into(),
            user_html: "<p>用户</p>".into(),
            answer_html: "<p>回答</p>".into(),
            attachments: vec![ShareAttachment {
                kind: ShareAttachmentKind::Image,
                display_name: "diagram.png".into(),
                url: "https://example.com/diagram.png".into(),
                content_type: Some("image/png".into()),
            }],
        });

        assert!(html.contains("diagram.png"));
        assert!(html.contains("https://example.com/diagram.png"));
    }
}
