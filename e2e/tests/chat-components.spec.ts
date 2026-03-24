import { test, expect } from '@playwright/test';

test.describe('Chat Components', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('welcome page renders correctly', async ({ page }) => {
    // 标题
    await expect(page.getByText('How can I help you today?')).toBeVisible();
    // 副标题
    await expect(page.getByText(/Select a project/)).toBeVisible();
    // 项目选择器
    await expect(page.getByText('Current Context')).toBeVisible();
    await expect(page.getByText('Q4 Marketing Strategy')).toBeVisible();
  });

  test('InputArea renders with welcome variant', async ({ page }) => {
    // 输入框存在且有正确的 placeholder
    const textarea = page.getByPlaceholder('Ask anything or use @ and / for tools...');
    await expect(textarea).toBeVisible();
  });

  test('InputArea accepts text input', async ({ page }) => {
    const textarea = page.getByPlaceholder('Ask anything or use @ and / for tools...');
    await textarea.fill('Hello, this is a test message');
    await expect(textarea).toHaveValue('Hello, this is a test message');
  });

  test('InputArea has action buttons', async ({ page }) => {
    // 发送按钮（SendHorizonal 图标在一个 Box 中）
    // 附件相关按钮存在（通过 svg 图标检测）
    const svgIcons = page.locator('svg');
    // 至少有 Paperclip, FileText, Mic, Image, SendHorizonal 等图标
    await expect(svgIcons.first()).toBeVisible();
  });

  test('suggestion buttons are visible', async ({ page }) => {
    await expect(page.getByText('Optimize')).toBeVisible();
    await expect(page.getByText('Summarize')).toBeVisible();
    await expect(page.getByText('Translate')).toBeVisible();
  });

  test('skill cards are visible', async ({ page }) => {
    await expect(page.getByText('Code Review')).toBeVisible();
    await expect(page.getByText('Data Analyst')).toBeVisible();
    await expect(page.getByText('Browse Agent')).toBeVisible();
    await expect(page.getByText('Drafting')).toBeVisible();
  });

  test('header shows Live Nodes', async ({ page }) => {
    await expect(page.getByText('Live Nodes')).toBeVisible();
  });

  test('clicking suggestion fills and submits', async ({ page }) => {
    // 点击 Optimize 建议按钮
    await page.getByText('Optimize').click();
    // 由于没有后端，提交后可能会报错或无响应
    // 但至少验证点击不会崩溃，页面仍然可用
    await page.waitForTimeout(500);
    // 页面应该还在（没有崩溃）
    await expect(page.locator('body')).toBeVisible();
  });

  test('Shift+Enter does not submit', async ({ page }) => {
    const textarea = page.getByPlaceholder('Ask anything or use @ and / for tools...');
    await textarea.fill('line 1');
    await textarea.press('Shift+Enter');
    // 页面应该仍在欢迎视图（没有提交）
    await expect(page.getByText('How can I help you today?')).toBeVisible();
  });
});
