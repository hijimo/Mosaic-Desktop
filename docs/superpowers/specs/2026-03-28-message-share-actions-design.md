# Message Share Actions Design

## Overview

This spec defines a new action bar rendered under assistant messages. The bar will include copy, thumbs up, thumbs down, and share actions.

The share action will not attempt direct platform integrations for DingTalk, WeChat, or Feishu. Instead, the desktop app will generate a standalone HTML share page, upload the page and its attachments to Alibaba Cloud OSS, and then invoke the operating system share sheet with the final public URL.

This keeps the UX consistent across platforms and avoids relying on application-specific desktop share APIs that are inconsistent or unavailable.

## Goals

- Add a message action bar under assistant messages.
- Support default copy and copy-as-Markdown.
- Support lightweight thumbs up / thumbs down UI state and animation.
- Support real share behavior by generating a standalone HTML page for a message and uploading related assets to OSS.
- Share the final OSS URL through the system share sheet, with clipboard fallback.

## Non-Goals

- No direct SDK integration for DingTalk, WeChat, or Feishu in v1.
- No persistence for thumbs up / thumbs down in v1.
- No full transcript export in v1.
- No sharing of internal reasoning, plans, or raw tool traces by default.

## Existing Context

Current message rendering is centered around:

- `/src/components/chat/Message.tsx`
- `/src/components/chat/MessageList.tsx`
- `/src/types/events.ts`

Current message data already contains enough structure to derive shareable content from `TurnGroup` and `TurnItem`.

The codebase currently does not contain:

- an existing message action bar,
- an OSS upload service,
- a Tauri share plugin integration,
- or a dedicated export/share content pipeline.

## Recommended Architecture

The feature will be split into four isolated parts:

1. **Message Action UI**
   - A front-end component rendered under assistant message content.
   - Handles copy, copy-as-Markdown, thumbs up, thumbs down, and share trigger.

2. **Share Content Mapper**
   - Converts a `TurnGroup` into a stable share domain model.
   - Decides which message items are included in the share page.

3. **Share HTML Renderer**
   - Produces a standalone HTML document from the share domain model.
   - References uploaded OSS asset URLs instead of local file paths.

4. **OSS Share Service**
   - Runs in Tauri Rust.
   - Uploads attachments first, then uploads the final HTML page.
   - Returns the public share URL to the front end.

This separation keeps UI logic, content mapping, HTML generation, and cloud upload concerns independent and testable.

## User Experience

Each assistant message block will expose four actions:

- **Copy**
  - Primary click performs plain-text copy.
  - Secondary menu offers `Copy` and `Copy as Markdown`.

- **Thumbs Up**
  - Front-end only state for v1.
  - Mutually exclusive with thumbs down.
  - Clicking the selected state clears it.

- **Thumbs Down**
  - Front-end only state for v1.
  - Mutually exclusive with thumbs up.
  - Clicking the selected state clears it.

- **Share**
  - Starts a background share job.
  - Shows progress state.
  - On success invokes the system share sheet with the final URL.
  - If the share sheet is unavailable or fails, the app copies the final URL to the clipboard and informs the user.

## Content Selection Rules

The share page is a content artifact, not a mirror of the runtime chat DOM.

### Included by default

- `UserMessage`
  - Text content is included.
  - User images and local images are treated as attachments.

- `AgentMessage`
  - Included as the main answer body.
  - Markdown semantics should be preserved.
  - Code blocks remain rendered as static code sections.

- `ImageView`
  - Included as an attachment or inline image block when possible.

### Excluded by default

- `Reasoning`
- `Plan`
- `CommandExecution`
- `McpToolCall`
- `DynamicToolCall`
- `WebSearch`
- `FileChange`
- `EnteredReviewMode`
- `ExitedReviewMode`
- `ContextCompaction`
- `CollabToolCall`

These items are excluded because they are either internal process artifacts, too noisy for external sharing, or may expose information not intended for recipients.

### Attachment handling

- Local images and local files must be uploaded to OSS before HTML generation is finalized.
- The HTML must reference the uploaded OSS URLs, never local filesystem paths.
- If an attachment cannot be read or uploaded, the share operation fails and no partial share page is published.

## Share Output Format

The generated artifact is a standalone HTML file with inline styles and no dependency on the running app bundle.

The page will contain:

- title,
- generation timestamp,
- user question section,
- assistant answer section,
- optional attachment section.

The page must be readable when opened independently in a browser and must not depend on MUI runtime styles, app CSS, or client-side JavaScript.

## OSS Upload Strategy

The share pipeline runs in Tauri Rust and uses an Alibaba Cloud OSS Rust SDK.

Recommended first implementation:

- use Rust-side configuration only,
- upload attachments first,
- generate HTML using final OSS URLs,
- upload HTML last,
- return the final public HTML URL.

OSS object key strategy:

- prefix with configured distribution path, such as `ai-share/`,
- create a unique share directory per action,
- store attachments under that directory,
- store the final page as `index.html`.

Example shape:

```text
ai-share/<share-id>/
ai-share/<share-id>/index.html
ai-share/<share-id>/assets/<file-name>
```

This layout keeps each shared message self-contained and easy to clean up later.

## Security and Configuration

OSS credentials must not be exposed to the React front end.

Implementation rules:

- React code never reads or handles OSS secrets.
- OSS configuration is loaded only inside the Tauri Rust process.
- Current `VITE_OSS_*` naming is not acceptable as a long-term state for production.
- The implementation should isolate OSS access behind a Rust service boundary so future migration to STS temporary credentials does not change the front-end API.

Operational note:

- The currently provided AccessKey should be rotated because it has already been pasted into the session.

## Front-End State Model

Three local state domains are required:

1. **Reaction state**
   - keyed by assistant message or turn group,
   - values: `up`, `down`, or `none`.

2. **Copy feedback state**
   - transient feedback for plain-text copy and Markdown copy.

3. **Share task state**
   - values: `idle`, `preparing`, `uploading`, `sharing`, `success`, `failed`.

This keeps fast UI interactions independent from the slower share pipeline.

## Failure Handling

- If attachment upload fails:
  - fail the share action,
  - show an error,
  - do not publish incomplete HTML.

- If HTML upload fails:
  - fail the share action,
  - do not invoke the system share sheet.

- If system share fails after upload succeeds:
  - copy the final URL to the clipboard,
  - notify the user that the link is ready for manual sharing.

- If a local file path is missing or unreadable:
  - fail fast with a clear error message.

## Testing Plan

### Front-end unit tests

- message action bar render conditions,
- copy plain text output,
- copy Markdown output,
- thumbs up / thumbs down exclusivity,
- share loading and failure states.

### Rust unit tests

- `TurnGroup` to share domain model conversion,
- HTML generation with OSS asset URL substitution,
- object key generation and upload sequencing.

### Integration tests

- attachments upload before HTML upload,
- final URL is returned only after all uploads succeed,
- share fallback copies URL when system share cannot be invoked.

### Manual verification

- pure text assistant message,
- code-heavy assistant message,
- assistant message with images,
- assistant message with local files,
- mixed-content message shared to DingTalk, WeChat, and Feishu through the system share sheet.

## Implementation Boundaries

The first implementation should stay narrowly scoped:

- only assistant messages get the action bar,
- only one share mode is exposed in UI,
- no user-configurable share template in v1,
- no persisted reaction history in v1,
- no public transcript archive of the entire conversation in v1.

## Open Decisions Resolved In This Spec

- Share target strategy: use system share sheet, not direct platform SDKs.
- Share payload strategy: upload attachments and a generated HTML page to OSS, then share the final HTML URL.
- Content inclusion strategy: include user question, assistant answer, and attachment-like assets; exclude internal process items by default.
- Reaction strategy: local-only UI state for v1.
- Copy strategy: plain text by default, Markdown via secondary menu.
