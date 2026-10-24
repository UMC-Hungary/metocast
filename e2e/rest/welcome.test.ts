/**
 * E2E tests for the welcome screen's settings, photos and logo (`/api/presenter/welcome`).
 * Requires the isolated test server (see vitest.config.ts) with TAURI_TEST_TOKEN set.
 */

import { describe, it, expect, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;
const BASE = process.env.TAURI_TEST_BASE_URL ?? 'http://localhost:3738';
const auth = { Authorization: `Bearer ${process.env.TAURI_TEST_TOKEN ?? ''}` };

interface WelcomeSettings {
  title: string;
  subtitle: string;
  churchName: string;
  language: string;
  hasLogo: boolean;
  replacedPhotos: number[];
}

describe.skipIf(!isLive)('Welcome screen REST API', () => {
  afterAll(async () => {
    await fetch(`${BASE}/api/presenter/welcome/photos/3`, { method: 'DELETE', headers: auth });
  });

  it('stores the text and language, and refuses an unknown language', async () => {
    const saved = await apiClient.put<WelcomeSettings>('/api/presenter/welcome', {
      title: 'Isten hozott!',
      subtitle: 'Az ÚR áldása legyen életeden!',
      churchName: 'E2E gyülekezet',
      language: 'hu',
    });
    expect(saved.status).toBe(200);
    expect(saved.body.churchName).toBe('E2E gyülekezet');
    const german = await apiClient.put('/api/presenter/welcome', {
      title: 'Willkommen',
      subtitle: '',
      churchName: '',
      language: 'de',
    });
    expect(german.status).toBe(400);
  });

  it('serves the built-in photos and takes a replacement back to the default', async () => {
    const photo = await fetch(`${BASE}/api/presenter/welcome/photos/3`, { headers: auth });
    expect(photo.status).toBe(200);
    expect(photo.headers.get('content-type')).toBe('image/jpeg');
    const bytes = new Uint8Array(await photo.arrayBuffer());

    const replaced = await fetch(`${BASE}/api/presenter/welcome/photos/3`, {
      method: 'PUT',
      headers: { ...auth, 'Content-Type': 'application/octet-stream' },
      body: bytes,
    });
    expect(replaced.status).toBe(204);
    const settings = await apiClient.get<WelcomeSettings>('/api/presenter/welcome');
    expect(settings.body.replacedPhotos).toContain(3);

    const reset = await fetch(`${BASE}/api/presenter/welcome/photos/3`, {
      method: 'DELETE',
      headers: auth,
    });
    expect(reset.status).toBe(204);
    const after = await apiClient.get<WelcomeSettings>('/api/presenter/welcome');
    expect(after.body.replacedPhotos).not.toContain(3);
  });

  it('previews a month as an SVG slide', async () => {
    const res = await fetch(`${BASE}/api/presenter/welcome/preview/10`, { headers: auth });
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toBe('image/svg+xml');
    expect(await res.text()).toContain('<svg');
  });

  it('refuses a photo outside 1-12 and a logo that is not PNG or SVG', async () => {
    const missing = await fetch(`${BASE}/api/presenter/welcome/photos/13`, { headers: auth });
    expect(missing.status).toBe(404);
    const logo = await fetch(`${BASE}/api/presenter/welcome/logo`, {
      method: 'PUT',
      headers: { ...auth, 'Content-Type': 'application/octet-stream' },
      body: 'not an image',
    });
    expect(logo.status).toBe(400);
  });

  it('keeps an SVG logo and removes it', async () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="red"/></svg>';
    const put = await fetch(`${BASE}/api/presenter/welcome/logo`, {
      method: 'PUT',
      headers: { ...auth, 'Content-Type': 'application/octet-stream' },
      body: svg,
    });
    expect(put.status).toBe(204);
    expect((await apiClient.get<WelcomeSettings>('/api/presenter/welcome')).body.hasLogo).toBe(
      true,
    );
    const removed = await fetch(`${BASE}/api/presenter/welcome/logo`, {
      method: 'DELETE',
      headers: auth,
    });
    expect(removed.status).toBe(204);
    expect((await apiClient.get<WelcomeSettings>('/api/presenter/welcome')).body.hasLogo).toBe(
      false,
    );
  });
});
