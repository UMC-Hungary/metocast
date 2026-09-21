<script lang="ts">
  import { pickDirectory, saveBrunoCollection, hostCapabilities } from '@metocast/core-client';
  import { serverUrl, serverPort, authToken, localNetworkUrl } from '$lib/stores/server-url.js';

  let copiedCurl = $state(false);

  function curlExample(): string {
    const base = $localNetworkUrl || $serverUrl;
    return `curl -H "Authorization: Bearer ${$authToken}" ${base}/api/events`;
  }

  async function copyCurl() {
    await navigator.clipboard.writeText(curlExample());
    copiedCurl = true;
    setTimeout(() => {
      copiedCurl = false;
    }, 2000);
  }

  function httpFile(
    name: string,
    seq: number,
    method: 'GET' | 'POST',
    path: string,
    body?: string,
  ): string {
    const lines = [
      `info:`,
      `  name: ${name}`,
      `  type: http`,
      `  seq: ${seq}`,
      ``,
      `http:`,
      `  method: ${method}`,
      `  url: "{{baseUrl}}${path}"`,
    ];
    if (body) {
      lines.push(`  body:`);
      lines.push(`    type: json`);
      lines.push(`    data: |-`);
      body.split('\n').forEach((line) => lines.push(`      ${line}`));
    }
    lines.push(
      `  auth:`,
      `    type: bearer`,
      `    token: "{{authToken}}"`,
      ``,
      `settings:`,
      `  encodeUrl: true`,
      `  timeout: 0`,
      `  followRedirects: true`,
    );
    return lines.join('\n');
  }

  function httpFilePublic(name: string, seq: number, path: string): string {
    return [
      `info:`,
      `  name: ${name}`,
      `  type: http`,
      `  seq: ${seq}`,
      ``,
      `http:`,
      `  method: GET`,
      `  url: "{{baseUrl}}${path}"`,
      `  auth:`,
      `    type: none`,
      ``,
      `settings:`,
      `  encodeUrl: true`,
      `  timeout: 0`,
      `  followRedirects: true`,
    ].join('\n');
  }

  function wsFile(): string {
    const wsBase = $localNetworkUrl
      ? $localNetworkUrl.replace('http', 'ws')
      : `ws://localhost:{{serverPort}}`;
    return [
      `info:`,
      `  name: WebSocket`,
      `  type: websocket`,
      `  seq: 7`,
      ``,
      `websocket:`,
      `  url: ${wsBase}/ws?token={{authToken}}`,
      `  message:`,
      `    type: json`,
      `    data: "{}"`,
      `  auth: inherit`,
      ``,
      `settings:`,
      `  timeout: 0`,
      `  keepAliveInterval: 0`,
    ].join('\n');
  }

  function envFile(): string {
    return [
      `name: Local`,
      `variables:`,
      `  - name: baseUrl`,
      `    value: "${$serverUrl}"`,
      `  - name: authToken`,
      `    value: "${$authToken}"`,
      `  - name: serverPort`,
      `    value: "${$serverPort}"`,
    ].join('\n');
  }

  async function saveBruno() {
    const dir = await pickDirectory('Choose folder for Bruno collection');
    if (!dir) return;

    const files: Record<string, string> = {
      'Metocast API/opencollection.yml': [
        `opencollection: 1.0.0`,
        ``,
        `info:`,
        `  name: Metocast API`,
        `bundled: false`,
        `extensions:`,
        `  bruno:`,
        `    ignore:`,
        `      - node_modules`,
        `      - .git`,
      ].join('\n'),
      'Metocast API/List Events.yml': httpFile('List Events', 1, 'GET', '/api/events'),
      'Metocast API/Get Event.yml': httpFile('Get Event', 2, 'GET', '/api/events/:id'),
      'Metocast API/Create Event.yml': httpFile(
        'Create Event',
        3,
        'POST',
        '/api/events',
        `{\n  "title": "Sunday Service",\n  "date_time": "${new Date().toISOString()}",\n  "speaker": "Pastor"\n}`,
      ),
      'Metocast API/List Recordings.yml': httpFile(
        'List Recordings',
        4,
        'GET',
        '/api/events/:id/recordings',
      ),
      'Metocast API/Add Recording.yml': httpFile(
        'Add Recording',
        5,
        'POST',
        '/api/events/:id/recordings',
        `{\n  "filename": "recording.mp4",\n  "duration_seconds": 3600\n}`,
      ),
      'Metocast API/Get Connector Statuses.yml': httpFile(
        'Get Connector Statuses',
        6,
        'GET',
        '/api/connectors/status',
      ),
      'Metocast API/WebSocket.yml': wsFile(),
      'Metocast API/OpenAPI Spec.yml': httpFilePublic('OpenAPI Spec', 8, '/openapi.json'),
      'Metocast API/environments/Local.yml': envFile(),
    };

    await saveBrunoCollection(dir, files);
  }
</script>

<div class="guide">
  <h3>API Examples</h3>

  <section>
    <h4>curl</h4>
    <pre><code>{curlExample()}</code></pre>
    <button onclick={copyCurl}>{copiedCurl ? 'Copied!' : 'Copy'}</button>
  </section>

  <section>
    <h4>WebSocket (connect from any client)</h4>
    {#if $localNetworkUrl}
      <pre><code>{$localNetworkUrl.replace('http', 'ws')}/ws?token={$authToken}</code></pre>
    {:else}
      <pre><code>ws://localhost:{$serverPort}/ws?token={$authToken}</code></pre>
    {/if}
  </section>

  <section>
    <h4>Bruno Collection</h4>
    <p class="hint">Choose a folder — a "Metocast API" collection will be created inside it.</p>
    {#if hostCapabilities.dialogs}
      <button onclick={saveBruno}>Save Bruno Collection…</button>
    {/if}
  </section>

  <section>
    <h4>Endpoint Reference</h4>
    <table>
      <thead>
        <tr>
          <th scope="col">Method</th>
          <th scope="col">Path</th>
          <th scope="col">Description</th>
        </tr>
      </thead>
      <tbody>
        <tr><td>GET</td><td>/api/events</td><td>List all events</td></tr>
        <tr><td>GET</td><td>/api/events/:id</td><td>Get event details</td></tr>
        <tr><td>POST</td><td>/api/events</td><td>Create event</td></tr>
        <tr><td>GET</td><td>/api/events/:id/recordings</td><td>List recordings</td></tr>
        <tr><td>POST</td><td>/api/events/:id/recordings</td><td>Add recording</td></tr>
        <tr
          ><td>GET</td><td>/api/connectors/status</td><td>Connector statuses (all connectors)</td
          ></tr
        >
        <tr><td>GET</td><td>/ws?token=…</td><td>WebSocket stream</td></tr>
        <tr><td>GET</td><td>/openapi.json</td><td>OpenAPI 3.1 spec (no auth)</td></tr>
        <tr><td>GET</td><td>/docs</td><td>Interactive API reference (no auth)</td></tr>
      </tbody>
    </table>
  </section>
</div>

<style>
  .guide {
    max-width: 700px;
  }

  .guide h3 {
    margin: 0 0 1.5rem;
  }

  section {
    margin-bottom: 1.5rem;
  }

  h4 {
    margin: 0 0 0.5rem;
    font-size: 0.875rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-secondary);
  }

  .hint {
    margin: 0 0 0.5rem;
    font-size: 0.8125rem;
    color: var(--text-secondary);
  }

  pre {
    padding: 0.75rem;
    background: var(--content-bg);
    border-radius: 0.375rem;
    overflow-x: auto;
    font-size: 0.875rem;
    margin-bottom: 0.5rem;
  }

  button {
    padding: 0.5rem 1rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: var(--glass-card-bg);
    cursor: pointer;
    font-size: 0.875rem;
  }

  button:hover {
    background: var(--nav-item-hover);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
  }

  th,
  td {
    padding: 0.5rem;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }

  th {
    font-weight: 600;
  }
</style>
