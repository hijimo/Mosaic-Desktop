#!/usr/bin/env node

import { spawnSync } from 'node:child_process';

function hasTauriDriver() {
  const result = spawnSync('which', ['tauri-driver'], {
    stdio: 'pipe',
    shell: process.platform === 'win32',
  });
  return result.status === 0;
}

if (hasTauriDriver()) {
  console.log('tauri-driver 已安装，跳过。');
  process.exit(0);
}

console.log('正在安装 tauri-driver...');
const install = spawnSync('cargo', ['install', 'tauri-driver', '--locked'], {
  stdio: 'inherit',
  shell: process.platform === 'win32',
});

if (install.status !== 0) {
  process.exit(install.status ?? 1);
}

console.log('tauri-driver 安装完成。');
