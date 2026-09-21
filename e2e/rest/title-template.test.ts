/**
 * E2E tests for the event title template setting, plus the renderer that
 * consumes it — the default template must produce the agreed title exactly.
 */

import { describe, it, expect, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';
import {
  renderTitle,
  DEFAULT_TITLE_TEMPLATE,
} from '../../packages/core-client/src/utils/title-template.js';

interface TitleTemplate {
  template: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

const values = {
  date: new Date(2026, 7, 9, 10, 0),
  title: '(Vasárnapi) istentisztelet',
  textus: 'Zsolt 128,1',
  leckio: '128. Zsolt',
  speaker: 'Prókai Árpád',
};

describe('title template renderer', () => {
  it('renders the agreed default title', () => {
    expect(renderTitle(DEFAULT_TITLE_TEMPLATE, values, 'hu')).toBe(
      '2026.08.09. (Vasárnapi) istentisztelet | Textus: Zsolt 128,1 Lekció: 128. Zsolt | Prókai Árpád',
    );
  });

  it('drops an optional group whose variable is empty, label and all', () => {
    expect(renderTitle(DEFAULT_TITLE_TEMPLATE, { ...values, leckio: '', speaker: '' }, 'hu')).toBe(
      '2026.08.09. (Vasárnapi) istentisztelet | Textus: Zsolt 128,1',
    );
  });

  it('formats dates through the pipe, longest token first', () => {
    expect(renderTitle('{date|YYYY. MMMM D., dddd}', values, 'hu')).toBe(
      '2026. augusztus 9., vasárnap',
    );
    expect(renderTitle('{date|YY-MM-DD HH:mm}', values, 'hu')).toBe('26-08-09 10:00');
  });
});

describe.skipIf(!isLive)('Title template REST API', () => {
  afterAll(async () => {
    await apiClient.put('/api/settings/title-template', { template: DEFAULT_TITLE_TEMPLATE });
  });

  it('GET → 200 with the default when unset', async () => {
    const res = await apiClient.get<TitleTemplate>('/api/settings/title-template');
    expect(res.status).toBe(200);
    expect(typeof res.body.template).toBe('string');
  });

  it('PUT round-trips a custom template', async () => {
    const custom = '{date|YYYY-MM-DD} — {title}';
    const put = await apiClient.put<TitleTemplate>('/api/settings/title-template', {
      template: custom,
    });
    expect(put.status).toBe(200);

    const get = await apiClient.get<TitleTemplate>('/api/settings/title-template');
    expect(get.body.template).toBe(custom);
  });

  it('the stored default matches the renderer default, so the two cannot drift', async () => {
    await apiClient.put('/api/settings/title-template', { template: DEFAULT_TITLE_TEMPLATE });
    const res = await apiClient.get<TitleTemplate>('/api/settings/title-template');
    expect(res.body.template).toBe(DEFAULT_TITLE_TEMPLATE);
  });
});
