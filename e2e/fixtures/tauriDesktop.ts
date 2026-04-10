import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';
import { expect, test as base } from '@playwright/test';
import { Builder, By, Key, until, type WebDriver, type WebElement } from 'selenium-webdriver';

type SupportedPlatform = NodeJS.Platform;

interface ResolveAppPathOptions {
  platform: SupportedPlatform;
  productName: string;
  targetDir: string;
}

interface DesktopFixture {
  app: TauriDesktopApp;
}

const DEFAULT_WEBDRIVER_URL = 'http://127.0.0.1:4444/';
const DEFAULT_WEBDRIVER_PORT = 4444;
const DEFAULT_TIMEOUT_MS = 15_000;

export function isDesktopWebDriverSupported(
  platform: SupportedPlatform = process.platform,
): boolean {
  return platform === 'linux' || platform === 'win32';
}

export function buildUnsupportedPlatformMessage(
  platform: SupportedPlatform = process.platform,
): string {
  if (platform === 'darwin') {
    return 'Tauri 官方桌面 WebDriver 当前不支持 macOS；请在 Windows / Linux 运行 tauri-driver E2E。';
  }
  return `当前平台 ${platform} 不在 Tauri 官方桌面 WebDriver 支持列表内；请在 Windows / Linux 运行 tauri-driver E2E。`;
}

export function resolveTauriAppPath({
  platform,
  productName,
  targetDir,
}: ResolveAppPathOptions): string {
  const executable = platform === 'win32' ? `${productName}.exe` : productName;
  const pathApi = platform === 'win32' ? path.win32 : path.posix;
  return pathApi.join(targetDir, 'debug', executable);
}

export function createTauriCapabilities(application: string): Record<string, unknown> {
  return {
    browserName: 'wry',
    'tauri:options': {
      application,
    },
  };
}

function commandExists(command: string): boolean {
  const pathDirs = (process.env.PATH ?? '').split(path.delimiter);
  return pathDirs.some((dir) => {
    if (!dir) return false;
    const candidate = path.join(dir, command);
    const candidateExe = path.join(dir, `${command}.exe`);
    return fs.existsSync(candidate) || fs.existsSync(candidateExe);
  });
}

function readProductName(rootDir: string): string {
  const tauriConfigPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
  const parsed = JSON.parse(fs.readFileSync(tauriConfigPath, 'utf-8')) as {
    productName?: string;
  };
  return parsed.productName ?? 'tauri-app';
}

async function waitForPort(port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    const connected = await new Promise<boolean>((resolve) => {
      const socket = net.createConnection({ host: '127.0.0.1', port }, () => {
        socket.end();
        resolve(true);
      });
      socket.on('error', () => {
        resolve(false);
      });
    });

    if (connected) return;
    await new Promise((resolve) => setTimeout(resolve, 150));
  }

  throw new Error(`Timed out waiting for WebDriver port ${port}`);
}

function resolveDesktopAppPath(rootDir: string): string {
  const productName = readProductName(rootDir);
  const targetDir = path.join(rootDir, 'src-tauri', 'target');
  const appPath = process.env.TAURI_APP_PATH
    ? path.resolve(process.env.TAURI_APP_PATH)
    : resolveTauriAppPath({
        platform: process.platform,
        productName,
        targetDir,
      });

  if (!fs.existsSync(appPath)) {
    throw new Error(
      `找不到 Tauri 可执行文件：${appPath}。请先运行 \`pnpm tauri build --debug --no-bundle\` 或通过 TAURI_APP_PATH 指定路径。`,
    );
  }

  return appPath;
}

function startTauriDriver(): ChildProcess {
  const driverBinary = process.env.TAURI_DRIVER_BINARY ?? 'tauri-driver';
  if (!commandExists(driverBinary)) {
    throw new Error(
      `未找到 ${driverBinary}。请先运行 \`pnpm install:tauri-driver\` 或自行安装 tauri-driver。`,
    );
  }

  const child = spawn(driverBinary, [], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: process.env,
  });

  child.stdout?.on('data', (chunk) => {
    process.stdout.write(`[tauri-driver] ${chunk}`);
  });
  child.stderr?.on('data', (chunk) => {
    process.stderr.write(`[tauri-driver] ${chunk}`);
  });

  return child;
}

function xpathText(text: string, exact = true): string {
  const escaped = text.includes('"')
    ? `concat("${text.split('"').join('", \'"\', "')}")`
    : `"${text}"`;
  return exact
    ? `//*[normalize-space() = ${escaped}]`
    : `//*[contains(normalize-space(), ${escaped})]`;
}

export class TauriDesktopApp {
  constructor(private readonly driver: WebDriver) {}

  async waitForReady(timeoutMs = DEFAULT_TIMEOUT_MS): Promise<void> {
    await this.driver.wait(async () => {
      const readyState = await this.driver.executeScript('return document.readyState');
      return readyState === 'complete' || readyState === 'interactive';
    }, timeoutMs);
    await this.waitForCss('body', timeoutMs);
  }

  async goto(route: string): Promise<void> {
    await this.driver.executeScript(
      `
        const nextPath = arguments[0];
        window.history.pushState({}, '', nextPath);
        window.dispatchEvent(new PopStateEvent('popstate'));
      `,
      route,
    );
  }

  async evaluate<T>(fn: string | ((...args: unknown[]) => T), ...args: unknown[]): Promise<T> {
    const script = typeof fn === 'string' ? fn : `return (${fn.toString()}).apply(null, arguments);`;
    return this.driver.executeScript(script, ...args) as Promise<T>;
  }

  async waitForText(text: string, timeoutMs = DEFAULT_TIMEOUT_MS, exact = true): Promise<WebElement> {
    const locator = By.xpath(xpathText(text, exact));
    await this.driver.wait(until.elementLocated(locator), timeoutMs);
    const element = await this.driver.findElement(locator);
    await this.driver.wait(until.elementIsVisible(element), timeoutMs);
    return element;
  }

  async countText(text: string, exact = true): Promise<number> {
    const elements = await this.driver.findElements(By.xpath(xpathText(text, exact)));
    return elements.length;
  }

  async clickText(text: string, exact = true): Promise<void> {
    const element = await this.waitForText(text, DEFAULT_TIMEOUT_MS, exact);
    await element.click();
  }

  async waitForCss(selector: string, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<WebElement> {
    const locator = By.css(selector);
    await this.driver.wait(until.elementLocated(locator), timeoutMs);
    return this.driver.findElement(locator);
  }

  async countCss(selector: string): Promise<number> {
    const elements = await this.driver.findElements(By.css(selector));
    return elements.length;
  }

  async fillByPlaceholder(placeholder: string, value: string): Promise<void> {
    const input = await this.driver.findElement(By.css(`[placeholder="${placeholder}"]`));
    await input.clear();
    await input.sendKeys(value);
  }

  async getValueByPlaceholder(placeholder: string): Promise<string> {
    const input = await this.driver.findElement(By.css(`[placeholder="${placeholder}"]`));
    return (await input.getAttribute('value')) ?? '';
  }

  async pressByPlaceholder(placeholder: string, key: string): Promise<void> {
    const input = await this.driver.findElement(By.css(`[placeholder="${placeholder}"]`));
    const translated = key === 'Enter' ? Key.ENTER : key === 'Shift+Enter' ? [Key.SHIFT, Key.ENTER] : key;
    if (Array.isArray(translated)) {
      await input.sendKeys(...translated);
      return;
    }
    await input.sendKeys(translated);
  }

  async quit(): Promise<void> {
    await this.driver.quit();
  }
}

export const test = base.extend<DesktopFixture>({
  app: [async ({}, use) => {
    if (!isDesktopWebDriverSupported()) {
      throw new Error(buildUnsupportedPlatformMessage());
    }

    const rootDir = process.cwd();
    const appPath = resolveDesktopAppPath(rootDir);
    const driverProcess = startTauriDriver();

    try {
      await waitForPort(DEFAULT_WEBDRIVER_PORT, DEFAULT_TIMEOUT_MS);
      const driver = await new Builder()
        .usingServer(process.env.TAURI_DRIVER_URL ?? DEFAULT_WEBDRIVER_URL)
        .forBrowser('wry')
        .withCapabilities(createTauriCapabilities(appPath) as never)
        .build();
      const app = new TauriDesktopApp(driver);
      await app.waitForReady();
      await use(app);
      await app.quit();
    } finally {
      driverProcess.kill();
    }
  }, { scope: 'test' }],
});

export { expect };
