import { test, expect } from '../fixtures/tauriDesktop';

test.describe('Core Tools Capability Alignment', () => {
  test('clarification card submits a structured user_input_answer op', async ({ app }) => {
    await app.goto('/thread/thread-e2e');

    await app.evaluate(() => {
      const bridge = (window as typeof window & {
        __MOSAIC_E2E__?: {
          reset: () => void;
          seedClarification: (request?: {
            threadId?: string;
            message?: string;
          }) => void;
        };
      }).__MOSAIC_E2E__;

      bridge?.reset();
      bridge?.seedClarification({
        threadId: 'thread-e2e',
        message: '请选择部署环境',
      });
    });

    await app.waitForText('需要澄清');
    await app.waitForText('请选择部署环境');
    await app.clickText('staging');
    await app.clickText('提交答案');

    await expect(await app.countText('需要澄清')).toBe(0);

    const submittedOps = await app.evaluate(() => {
      const bridge = (window as typeof window & {
        __MOSAIC_E2E__?: {
          getSubmittedOps: () => Array<{
            threadId: string;
            id: string;
            op: {
              type: string;
              id: string;
              response: {
                answers: Array<{
                  question: string;
                  choice: string;
                  source: string;
                }>;
              };
            };
          }>;
        };
      }).__MOSAIC_E2E__;

      return bridge?.getSubmittedOps() ?? [];
    });

    expect(submittedOps).toHaveLength(1);
    expect(submittedOps[0]).toMatchObject({
      threadId: 'thread-e2e',
      op: {
        type: 'user_input_answer',
        id: 'clarify-e2e',
        response: {
          answers: [
            {
              question: '部署环境',
              choice: 'staging',
              source: 'option',
            },
          ],
        },
      },
    });
  });
});
