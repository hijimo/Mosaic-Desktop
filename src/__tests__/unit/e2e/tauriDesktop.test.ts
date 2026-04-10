// @vitest-environment node

import { describe, expect, it } from 'vitest';
import {
  buildUnsupportedPlatformMessage,
  createTauriCapabilities,
  isDesktopWebDriverSupported,
  resolveTauriAppPath,
} from '../../../../e2e/fixtures/tauriDesktop';

describe('tauriDesktop helpers', () => {
  it('identifies supported desktop webdriver platforms', () => {
    expect(isDesktopWebDriverSupported('linux')).toBe(true);
    expect(isDesktopWebDriverSupported('win32')).toBe(true);
    expect(isDesktopWebDriverSupported('darwin')).toBe(false);
  });

  it('resolves the debug app path for unix targets', () => {
    expect(
      resolveTauriAppPath({
        platform: 'linux',
        productName: 'tauri-app',
        targetDir: '/tmp/src-tauri/target',
      }),
    ).toBe('/tmp/src-tauri/target/debug/tauri-app');
  });

  it('resolves the debug app path for windows targets', () => {
    expect(
      resolveTauriAppPath({
        platform: 'win32',
        productName: 'tauri-app',
        targetDir: 'C:\\repo\\src-tauri\\target',
      }),
    ).toBe('C:\\repo\\src-tauri\\target\\debug\\tauri-app.exe');
  });

  it('builds tauri webdriver capabilities', () => {
    expect(createTauriCapabilities('/tmp/tauri-app')).toEqual({
      browserName: 'wry',
      'tauri:options': {
        application: '/tmp/tauri-app',
      },
    });
  });

  it('explains the macOS desktop webdriver limitation clearly', () => {
    expect(buildUnsupportedPlatformMessage('darwin')).toContain('macOS');
    expect(buildUnsupportedPlatformMessage('darwin')).toContain('Windows / Linux');
  });
});
