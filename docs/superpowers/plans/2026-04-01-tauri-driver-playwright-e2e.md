# Tauri Driver Playwright E2E Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Mosaic 前端 E2E 自动化测试栈升级为“`tauri-driver` + Playwright runner”，并补齐运行脚本、桌面测试夹具与文档。

**Architecture:** 保留 Playwright 作为测试 runner、断言与报告层，引入 `tauri-driver` 提供桌面 WebView 的 WebDriver 桥接；测试不再依赖浏览器 `page` fixture，而是通过一个桌面驱动夹具与 Tauri 应用交互。继续复用现有 `window.__MOSAIC_E2E__` bridge 作为测试注入/观测口，避免把测试专用逻辑扩散到生产代码。

**Tech Stack:** Tauri v2、`tauri-driver`、Playwright Test、`selenium-webdriver`、TypeScript、Node.js 脚本、现有 `e2e/tests/*` 与 `src/App.tsx` E2E bridge

---

## File Structure

- Create: `scripts/e2e/install-tauri-driver.mjs`
  - 安装/检查 `tauri-driver` 的本地脚本。
- Create: `scripts/e2e/desktop-e2e.mjs`
  - 统一启动桌面 E2E 的入口脚本。
- Create: `e2e/fixtures/tauriDesktop.ts`
  - Playwright runner 用的桌面驱动 fixture。
- Create: `e2e/fixtures/tauriDesktop.test.ts`
  - fixture 辅助函数单测。
- Modify: `playwright.config.ts`
  - 改成桌面 E2E 配置，不再使用纯 `webServer` 浏览器模式。
- Modify: `package.json`
  - 增加安装/运行 `tauri-driver` 的脚本，调整默认 E2E 命令。
- Modify: `e2e/tests/chat-components.spec.ts`
  - 从 `page` 迁移到桌面驱动 fixture。
- Modify: `e2e/tests/core-tools-capability-alignment.spec.ts`
  - 从 `page` 迁移到桌面驱动 fixture。
- Modify: `src/App.tsx`
  - 只做最小 bridge 稳定性增强，保持桌面注入可用。
- Modify: `docs/frontend/02-testing-strategy.md`
  - 更新前端测试技术栈与运行方式。
- Modify: `docs/tauri-testing-plan.md`
  - 更新 Tauri 自动化测试方案与平台限制说明。

## Task 1: 搭建可测试的桌面 E2E 基础设施

**Files:**
- Create: `e2e/fixtures/tauriDesktop.ts`
- Create: `e2e/fixtures/tauriDesktop.test.ts`
- Modify: `playwright.config.ts`

- [ ] 写 fixture 辅助函数失败测试，覆盖平台判断、应用路径解析、能力参数拼装。
- [ ] 运行失败测试，确认当前仓库还没有桌面 E2E fixture。
- [ ] 实现最小 fixture：启动 `tauri-driver`、启动已构建 Tauri app、建立 WebDriver session。
- [ ] 运行 fixture 单测，确认通过。

## Task 2: 安装并接入 tauri-driver

**Files:**
- Create: `scripts/e2e/install-tauri-driver.mjs`
- Create: `scripts/e2e/desktop-e2e.mjs`
- Modify: `package.json`

- [ ] 增加失败测试或脚本断言，要求 `tauri-driver` 缺失时给出清晰错误信息。
- [ ] 本机安装 `tauri-driver`，并将安装检查脚本纳入仓库。
- [ ] 增加 `pnpm test:e2e` / `pnpm test:e2e:desktop` / `pnpm install:tauri-driver` 等脚本。
- [ ] 在 macOS 上明确 fail-fast，提示 Tauri 官方桌面 WebDriver 仅支持 Windows / Linux。

## Task 3: 迁移现有前端 E2E 用例

**Files:**
- Modify: `e2e/tests/chat-components.spec.ts`
- Modify: `e2e/tests/core-tools-capability-alignment.spec.ts`
- Modify: `src/App.tsx`

- [ ] 先把一个用例改写成桌面驱动版失败测试。
- [ ] 跑失败测试，确认旧的 `page` fixture 路径不再适用。
- [ ] 实现桌面驱动 helper（查找文本、placeholder、点击、输入、执行 bridge 脚本）。
- [ ] 迁移剩余用例，尽量保留原断言语义。
- [ ] 跑迁移后的测试；在不支持的平台上验证错误信息正确。

## Task 4: 更新文档

**Files:**
- Modify: `docs/frontend/02-testing-strategy.md`
- Modify: `docs/tauri-testing-plan.md`

- [ ] 更新“工具选型”“目录结构”“运行命令”“平台限制”。
- [ ] 明确 Playwright 在这里承担 runner/reporting，`tauri-driver` 承担桌面 WebDriver transport。
- [ ] 写清本地 macOS 不支持桌面 WebDriver，推荐在 Linux / Windows CI 运行。

## Task 5: 最终验证

**Files:**
- Test: `e2e/fixtures/tauriDesktop.test.ts`
- Test: `e2e/tests/chat-components.spec.ts`
- Test: `e2e/tests/core-tools-capability-alignment.spec.ts`

- [ ] 运行相关单测与 TypeScript 检查。
- [ ] 运行安装脚本自检。
- [ ] 运行桌面 E2E 命令，记录在当前平台的真实结果。
- [ ] 在最终说明中区分“代码已接好”和“当前机器能否实际跑桌面 E2E”。
