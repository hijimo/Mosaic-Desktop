import { describe, expect, it, vi } from 'vitest';
import type { TurnGroup } from '@/types';
import {
  buildCopyMarkdown,
  buildCopyText,
  buildSharePayload,
} from '@/components/chat/agent/messageShareContent';

const fixedDate = new Date('2026-03-28T10:00:00.000Z');

const group: TurnGroup = {
  turn_id: 'turn-share-1',
  items: [
    {
      type: 'UserMessage',
      id: 'user-1',
      content: [
        { type: 'text', text: '请总结下面这段代码', text_elements: [] },
        { type: 'local_image', path: 'C:\\tmp\\request.png' },
      ],
    },
    {
      type: 'AgentMessage',
      id: 'agent-1',
      content: [{ type: 'Text', text: '这里是回答。\n\n```ts\nconsole.log(1)\n```' }],
    },
    {
      type: 'ImageView',
      id: 'img-1',
      path: '/tmp/result.png',
    },
    {
      type: 'Reasoning',
      id: 'reason-1',
      summary_text: ['不会出现在分享页'],
      raw_content: [],
    },
  ],
};

describe('messageShareContent', () => {
  it('builds plain text for direct copy', () => {
    expect(buildCopyText(group)).toContain('用户');
    expect(buildCopyText(group)).toContain('助手');
    expect(buildCopyText(group)).toContain('这里是回答。');
    expect(buildCopyText(group)).not.toContain('不会出现在分享页');
  });

  it('builds markdown for markdown copy', () => {
    const markdown = buildCopyMarkdown(group);
    expect(markdown).toContain('## 用户问题');
    expect(markdown).toContain('## 助手回答');
    expect(markdown).toContain('```ts');
    expect(markdown).toContain('![request.png]');
  });

  it('builds normalized share payload with attachments', () => {
    vi.useFakeTimers();
    vi.setSystemTime(fixedDate);

    const payload = buildSharePayload(group);

    expect(payload.turnId).toBe('turn-share-1');
    expect(payload.generatedAt).toBe(fixedDate.toISOString());
    expect(payload.userText).toContain('请总结下面这段代码');
    expect(payload.answerMarkdown).toContain('console.log(1)');
    expect(payload.attachments).toEqual([
      expect.objectContaining({
        kind: 'image',
        sourcePath: 'C:\\tmp\\request.png',
        displayName: 'request.png',
      }),
      expect.objectContaining({
        kind: 'image',
        sourcePath: '/tmp/result.png',
        displayName: 'result.png',
      }),
    ]);

    vi.useRealTimers();
  });
});
