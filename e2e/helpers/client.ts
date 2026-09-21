/**
 * HTTP API client helper for E2E tests.
 *
 * Requires:
 *   - Server running at http://localhost:3737
 *   - TAURI_TEST_TOKEN env var set to a valid bearer token
 */

const BASE_URL = process.env.TAURI_TEST_BASE_URL ?? 'http://localhost:3737';
const TOKEN = process.env.TAURI_TEST_TOKEN ?? '';

type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE';

interface ApiRequestOptions {
  method?: HttpMethod;
  body?: Record<string, unknown>;
  query?: Record<string, string>;
}

interface ApiResponse<T> {
  status: number;
  body: T;
}

async function request<T>(
  path: string,
  { method = 'GET', body, query }: ApiRequestOptions = {},
): Promise<ApiResponse<T>> {
  let url = `${BASE_URL}${path}`;
  if (query) {
    const params = new URLSearchParams(query);
    url += `?${params.toString()}`;
  }

  const res = await fetch(url, {
    method,
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      'Content-Type': 'application/json',
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  let responseBody: T;
  const contentType = res.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    responseBody = (await res.json()) as T;
  } else {
    responseBody = (await res.text()) as T;
  }

  return { status: res.status, body: responseBody };
}

export const apiClient = {
  get: <T>(path: string, query?: Record<string, string>) =>
    request<T>(path, { method: 'GET', query }),
  post: <T>(path: string, body?: Record<string, unknown>) =>
    request<T>(path, { method: 'POST', body }),
  put: <T>(path: string, body?: Record<string, unknown>) =>
    request<T>(path, { method: 'PUT', body }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
};
