/**
 * E2E tests for the Bible REST API.
 *
 * The passage/suggest endpoints call third-party services, so only the parts that
 * never leave the core are asserted unconditionally. Set BIBLE_UPSTREAM=1 to also
 * exercise the live upstream lookups.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface BiblePassage {
  label: string;
  verses: { chapter: number; verse: number; text: string }[];
}

interface BibleSuggestion {
  cat: string;
  label: string;
  link: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;
const hitsUpstream = process.env.BIBLE_UPSTREAM === '1';

describe.skipIf(!isLive)('Bible REST API', () => {
  it('GET /api/bible/suggest with a 1-character term → empty list, no upstream call', async () => {
    const res = await apiClient.get<BibleSuggestion[]>('/api/bible/suggest', { term: 'J' });
    expect(res.status).toBe(200);
    expect(res.body).toEqual([]);
  });

  it('GET /api/bible/verses without params → 400', async () => {
    const res = await apiClient.get('/api/bible/verses');
    expect(res.status).toBe(400);
  });

  it.runIf(hitsUpstream)('GET /api/bible/verses → normalised passage', async () => {
    // RUF_v2 routes to the V2 API. The legacy verse API (szentiras.eu
    // /api/idezet) started requiring an X-API-Key, so it is not covered here.
    const res = await apiClient.get<BiblePassage>('/api/bible/verses', {
      reference: 'Jn 3,16',
      translation: 'RUF_v2',
    });
    expect(res.status).toBe(200);
    expect(res.body.verses.length).toBeGreaterThan(0);
    expect(res.body.verses[0]).toMatchObject({
      chapter: expect.any(Number),
      verse: expect.any(Number),
      text: expect.any(String),
    });
    // Markup is stripped by the core.
    expect(res.body.verses[0]?.text).not.toMatch(/<[^>]+>/);
  });

  it.runIf(hitsUpstream)('GET /api/bible/suggest → reference suggestions', async () => {
    const res = await apiClient.get<BibleSuggestion[]>('/api/bible/suggest', { term: 'Jn' });
    expect(res.status).toBe(200);
    expect(res.body.every((s) => s.cat === 'ref')).toBe(true);
  });
});
