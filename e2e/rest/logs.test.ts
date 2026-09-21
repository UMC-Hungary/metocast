/**
 * E2E tests for the application log endpoint. Clearing is deliberately not
 * exercised — it would wipe the log of whatever core the suite runs against.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface ApplicationLog {
  path: string;
  content: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;
const baseUrl = process.env.TAURI_TEST_BASE_URL ?? 'http://localhost:3737';

describe.skipIf(!isLive)('Application log REST API', () => {
  // The headless binary logs to stdout and owns no log file, so it answers 503
  // by design; a desktop core hands back the file it writes.
  it('GET → the log path and contents, or 503 on a core without a log file', async () => {
    const res = await apiClient.get<ApplicationLog>('/api/logs');
    expect([200, 503]).toContain(res.status);
    if (res.status === 200) {
      expect(res.body.path).toContain('metocast.log');
      expect(typeof res.body.content).toBe('string');
    }
  });

  it('rejects a request without a token, so a client needs one to read the log', async () => {
    const res = await fetch(`${baseUrl}/api/logs`);
    expect(res.status).toBe(401);
  });
});
