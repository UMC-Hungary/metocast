import type { z } from 'zod';
import { coreConfig } from '../config.js';

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

type RequestOptions = Omit<RequestInit, 'headers' | 'body'> & {
  body?: object;
  headers?: Record<string, string>;
};

async function waitForToken(): Promise<string> {
  const start = Date.now();
  for (;;) {
    const token = coreConfig().authToken;
    if (token) return token;
    if (Date.now() - start >= 10_000) throw new Error('Timed out waiting for auth token');
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/**
 * The desktop app boots its server asynchronously, so the first requests after
 * launch can arrive before the listener is up. Only connection failures are
 * retried — an HTTP response, including an error status, is returned as-is.
 */
async function fetchWhenServerUp(url: string, init: RequestInit): Promise<Response> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await fetch(url, init);
    } catch (e) {
      if (attempt >= 10) throw e;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }
}

export async function apiFetch<S extends z.ZodType>(
  path: string,
  schema: S,
  options: RequestOptions = {},
): Promise<z.infer<S>> {
  const base = coreConfig().serverUrl;
  const token = await waitForToken();
  const { body, headers, ...restOptions } = options;

  const init: RequestInit = {
    ...restOptions,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
      ...headers,
    },
  };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }

  const res = await fetchWhenServerUp(`${base}${path}`, init);

  if (!res.ok) {
    throw new ApiError(res.status, `API error ${res.status}: ${await res.text()}`);
  }

  if (res.status === 204) {
    return schema.parse(undefined) as z.infer<S>;
  }
  const data = await res.json();
  return schema.parse(data) as z.infer<S>;
}

/** Outcome of probing a core the UI is not yet configured against. */
export type CoreProbe =
  | { ok: true }
  | { ok: false; reason: 'unreachable' | 'unauthorized' | 'unexpected'; status?: number };

/**
 * Checks that a URL really is a Metocast core and that the token is accepted,
 * before the setup flow commits to it. Deliberately bypasses the configured
 * server URL and token, which is exactly what makes it part of the SDK rather
 * than a raw fetch in a component.
 */
export async function probeCore(url: string, token: string): Promise<CoreProbe> {
  let response: Response;
  try {
    response = await fetch(`${url}/api/events`, {
      headers: { Authorization: `Bearer ${token}` },
    });
  } catch {
    return { ok: false, reason: 'unreachable' };
  }
  if (response.status === 401) return { ok: false, reason: 'unauthorized' };
  if (!response.ok) return { ok: false, reason: 'unexpected', status: response.status };
  return { ok: true };
}
