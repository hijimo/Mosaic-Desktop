#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

function isSupportedDesktopWebDriverPlatform(platform = process.platform) {
  return platform === 'linux' || platform === 'win32';
}

function unsupportedPlatformMessage(platform = process.platform) {
  if (platform === 'darwin') {
    return '当前机器是 macOS。Tauri 官方桌面 WebDriver 仅支持 Windows / Linux，无法在本机运行 tauri-driver 桌面 E2E。';
  }
  return `当前平台 ${platform} 不支持 tauri-driver 桌面 E2E。`;
}

function readProductName(rootDir) {
  const tauriConfigPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
  const parsed = JSON.parse(fs.readFileSync(tauriConfigPath, 'utf-8'));
  return parsed.productName ?? 'tauri-app';
}

function resolveBuiltAppPath(rootDir) {
  const productName = readProductName(rootDir);
  const executable = process.platform === 'win32' ? `${productName}.exe` : productName;
  return path.join(rootDir, 'src-tauri', 'target', 'debug', executable);
}

function run(command, args, extraEnv = {}) {
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    shell: process.platform === 'win32',
    env: {
      ...process.env,
      ...extraEnv,
    },
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

if (!isSupportedDesktopWebDriverPlatform()) {
  console.error(unsupportedPlatformMessage());
  process.exit(1);
}

const rootDir = process.cwd();
run('node', ['./scripts/e2e/install-tauri-driver.mjs']);
run('pnpm', ['tauri', 'build', '--debug', '--no-bundle']);

const appPath = resolveBuiltAppPath(rootDir);
if (!fs.existsSync(appPath)) {
  console.error(`构建完成后仍未找到桌面可执行文件：${appPath}`);
  process.exit(1);
}

const forwardedArgs = process.argv.slice(2);
run('pnpm', ['playwright', 'test', '--config', 'playwright.config.ts', ...forwardedArgs], {
  TAURI_APP_PATH: appPath,
});
