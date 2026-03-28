import type { TurnGroup, TurnItem, UserInput } from '@/types';

export interface ShareAttachmentInput {
  kind: 'image' | 'file';
  sourcePath: string;
  displayName: string;
  contentType?: string;
}

export interface ShareMessagePayload {
  turnId: string;
  title: string;
  generatedAt: string;
  userText: string;
  answerMarkdown: string;
  attachments: ShareAttachmentInput[];
}

export function buildCopyText(group: TurnGroup): string {
  const userText = collectUserText(group);
  const answerMarkdown = collectAnswerMarkdown(group).trim();

  return [`用户\n${userText}`, `助手\n${answerMarkdown}`]
    .map((section) => section.trim())
    .filter(Boolean)
    .join('\n\n');
}

export function buildCopyMarkdown(group: TurnGroup): string {
  const payload = buildSharePayload(group);
  const attachmentLines = payload.attachments.map((attachment) => {
    if (attachment.kind === 'image') {
      return `![${attachment.displayName}](${attachment.sourcePath})`;
    }

    return `- [${attachment.displayName}](${attachment.sourcePath})`;
  });

  return [
    '# 对话分享',
    '',
    '## 用户问题',
    payload.userText,
    '',
    '## 助手回答',
    payload.answerMarkdown,
    ...(attachmentLines.length > 0 ? ['', '## 附件', ...attachmentLines] : []),
  ]
    .join('\n')
    .trim();
}

export function buildSharePayload(group: TurnGroup): ShareMessagePayload {
  const userText = collectUserText(group);
  const answerMarkdown = collectAnswerMarkdown(group);

  return {
    turnId: group.turn_id,
    title: buildShareTitle(userText, group.turn_id),
    generatedAt: new Date().toISOString(),
    userText,
    answerMarkdown,
    attachments: collectAttachments(group),
  };
}

function collectUserText(group: TurnGroup): string {
  return group.items
    .filter(isUserMessage)
    .flatMap((item) => item.content)
    .filter(isTextInput)
    .map((input) => input.text.trim())
    .filter(Boolean)
    .join('\n\n');
}

function collectAnswerMarkdown(group: TurnGroup): string {
  return group.items
    .filter(isAgentMessage)
    .flatMap((item) => item.content)
    .map((content) => content.text)
    .join('')
    .trim();
}

function collectAttachments(group: TurnGroup): ShareAttachmentInput[] {
  const attachments: ShareAttachmentInput[] = [];

  for (const item of group.items) {
    if (item.type === 'UserMessage') {
      for (const input of item.content) {
        if (input.type !== 'local_image') {
          continue;
        }

        attachments.push({
          kind: 'image',
          sourcePath: input.path,
          displayName: fileNameOf(input.path),
          contentType: inferContentType(input.path),
        });
      }
    }

    if (item.type === 'ImageView') {
      attachments.push({
        kind: 'image',
        sourcePath: item.path,
        displayName: fileNameOf(item.path),
        contentType: inferContentType(item.path),
      });
    }
  }

  return attachments;
}

function buildShareTitle(userText: string, turnId: string): string {
  const firstLine = userText.split('\n').map((line) => line.trim()).find(Boolean);
  if (!firstLine) {
    return `对话分享 ${turnId}`;
  }

  return firstLine.length > 32 ? `${firstLine.slice(0, 32)}...` : firstLine;
}

function fileNameOf(path: string): string {
  const normalizedPath = path.split('\\').join('/');
  const segments = normalizedPath.split('/');
  return segments[segments.length - 1] || 'attachment';
}

function inferContentType(path: string): string | undefined {
  const extension = fileNameOf(path).split('.').pop()?.toLowerCase();
  switch (extension) {
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'webp':
      return 'image/webp';
    default:
      return undefined;
  }
}

function isUserMessage(item: TurnItem): item is Extract<TurnItem, { type: 'UserMessage' }> {
  return item.type === 'UserMessage';
}

function isAgentMessage(item: TurnItem): item is Extract<TurnItem, { type: 'AgentMessage' }> {
  return item.type === 'AgentMessage';
}

function isTextInput(input: UserInput): input is Extract<UserInput, { type: 'text' }> {
  return input.type === 'text';
}
