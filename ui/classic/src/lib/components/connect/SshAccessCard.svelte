<script lang="ts">
  import { untrack } from 'svelte';
  import { browser } from '$app/environment';
  import { connectedClients } from '$lib/stores/presenter.js';

  type SshDevice = {
    id: string;
    label: string;
    host: string;
    port: number;
    user: string;
    useKey: boolean;
    keyPath: string;
  };

  function makeDevice(partial: Partial<SshDevice> = {}): SshDevice {
    return {
      id: crypto.randomUUID(),
      label: '',
      host: '',
      port: 22,
      user: 'pi',
      useKey: false,
      keyPath: '~/.ssh/id_rsa',
      ...partial,
    };
  }

  function loadDevices(): SshDevice[] {
    if (!browser) return [makeDevice()];
    try {
      const raw = localStorage.getItem('ssh-devices');
      if (raw) {
        const parsed = JSON.parse(raw) as SshDevice[];
        if (Array.isArray(parsed) && parsed.length > 0) return parsed;
      }
    } catch {}
    return [makeDevice()];
  }

  let devices = $state<SshDevice[]>(loadDevices());

  $effect(() => {
    if (browser) localStorage.setItem('ssh-devices', JSON.stringify(devices));
  });

  // Suggest adding connected Pi devices not yet in the list
  const suggestions = $derived(
    $connectedClients.filter(
      (c) =>
        c.label === 'Presenter Receiver' &&
        c.hostname &&
        !untrack(() => devices.some((d) => d.host === c.hostname)),
    ),
  );

  function addSuggestion(hostname: string) {
    devices = [...devices, makeDevice({ host: hostname, label: hostname })];
  }

  function addDevice() {
    devices = [...devices, makeDevice()];
  }

  function removeDevice(id: string) {
    if (devices.length === 1) {
      devices = [makeDevice()];
    } else {
      devices = devices.filter((d) => d.id !== id);
    }
  }

  function sshCommand(device: SshDevice): string {
    if (!device.host.trim()) return '';
    const parts = ['ssh'];
    if (device.useKey) parts.push('-i', device.keyPath.trim() || '~/.ssh/id_rsa');
    if (device.port !== 22) parts.push('-p', String(device.port));
    parts.push(`${device.user.trim() || 'pi'}@${device.host.trim()}`);
    return parts.join(' ');
  }

  let copiedId = $state<string | null>(null);
  let copiedLogs = $state(false);

  const logCommand = 'tail -f ~/.local/log/presenter-receiver.log';

  async function copyCommand(device: SshDevice) {
    const cmd = sshCommand(device);
    if (!cmd) return;
    await navigator.clipboard.writeText(cmd);
    copiedId = device.id;
    setTimeout(() => {
      copiedId = null;
    }, 2000);
  }

  async function copyLogs() {
    await navigator.clipboard.writeText(logCommand);
    copiedLogs = true;
    setTimeout(() => {
      copiedLogs = false;
    }, 2000);
  }
</script>

<section class="info-card">
  <div class="card-header">
    <h2>SSH Access</h2>
    <button class="btn-add" onclick={addDevice}>+ Add device</button>
  </div>
  <p class="note">
    Save your remote devices here to quickly generate an SSH command for troubleshooting. Devices
    are stored locally in your browser. Once connected, run
    <code class="inline-code">tail -f ~/.local/log/presenter-receiver.log</code>
    to stream live logs from the presenter receiver.
  </p>

  {#if suggestions.length > 0}
    <div class="suggestions">
      {#each suggestions as s (s.id)}
        <button class="suggestion-chip" onclick={() => addSuggestion(s.hostname!)}>
          + {s.hostname}
        </button>
      {/each}
      <span class="suggestion-hint">detected from connected clients</span>
    </div>
  {/if}

  <div class="device-list">
    {#each devices as device (device.id)}
      <div class="device-card">
        <div class="device-header">
          <input
            class="device-label-input"
            type="text"
            placeholder="Label (optional)"
            bind:value={device.label}
            aria-label="Device label"
          />
          <button
            class="btn-remove"
            onclick={() => removeDevice(device.id)}
            aria-label="Remove device">✕</button
          >
        </div>

        <div class="ssh-form">
          <div class="ssh-row">
            <label class="ssh-label" for="host-{device.id}">Host</label>
            <input
              id="host-{device.id}"
              class="ssh-input ssh-input--grow"
              type="text"
              placeholder="192.168.0.40 or raspberrypi"
              bind:value={device.host}
            />
            <label class="ssh-label" for="port-{device.id}">Port</label>
            <input
              id="port-{device.id}"
              class="ssh-input ssh-input--port"
              type="number"
              min="1"
              max="65535"
              bind:value={device.port}
            />
          </div>

          <div class="ssh-row">
            <label class="ssh-label" for="user-{device.id}">User</label>
            <input
              id="user-{device.id}"
              class="ssh-input"
              type="text"
              placeholder="pi"
              bind:value={device.user}
            />
            <label class="ssh-auth-toggle">
              <input type="checkbox" bind:checked={device.useKey} />
              SSH key
            </label>
          </div>

          {#if device.useKey}
            <div class="ssh-row">
              <label class="ssh-label" for="key-{device.id}">Key</label>
              <input
                id="key-{device.id}"
                class="ssh-input ssh-input--grow"
                type="text"
                placeholder="~/.ssh/id_rsa"
                bind:value={device.keyPath}
              />
            </div>
          {/if}
        </div>

        <div class="cmd-row">
          {#if sshCommand(device)}
            <code class="cmd">{sshCommand(device)}</code>
            <button onclick={() => copyCommand(device)}>
              {copiedId === device.id ? 'Copied!' : 'Copy'}
            </button>
          {:else}
            <span class="note">Enter a host to generate the command.</span>
          {/if}
        </div>
      </div>
    {/each}
  </div>

  <h3>Stream logs on remote</h3>
  <div class="cmd-row">
    <code class="cmd">{logCommand}</code>
    <button onclick={copyLogs}>{copiedLogs ? 'Copied!' : 'Copy'}</button>
  </div>
</section>

<style>
  .info-card {
    padding: 1.25rem;
    background: var(--content-bg);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    max-width: 600px;
  }

  .card-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

  h2 {
    margin: 0;
  }

  h3 {
    margin: 1.25rem 0 0.25rem;
    font-size: 0.95rem;
  }

  .btn-add {
    padding: 0.2rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: var(--glass-card-bg);
    cursor: pointer;
    font-size: 0.8rem;
  }

  .btn-add:hover {
    background: var(--nav-item-hover);
  }

  .suggestions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-bottom: 0.75rem;
  }

  .suggestion-chip {
    padding: 0.15rem 0.55rem;
    border: 1px dashed var(--border);
    border-radius: 0.375rem;
    background: transparent;
    cursor: pointer;
    font-size: 0.78rem;
    font-family: monospace;
    color: var(--text-secondary);
  }

  .suggestion-chip:hover {
    background: var(--nav-item-hover);
    color: inherit;
  }

  .suggestion-hint {
    font-size: 0.75rem;
    color: var(--text-secondary, #666);
  }

  .device-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin-bottom: 0.25rem;
  }

  .device-card {
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .device-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .device-label-input {
    flex: 1;
    padding: 0.2rem 0.5rem;
    border: none;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    background: transparent;
    font-size: 0.85rem;
    font-weight: 600;
    color: inherit;
    min-width: 0;
  }

  .device-label-input:focus {
    outline: none;
    border-bottom-color: var(--nav-item-active-bg);
  }

  .device-label-input::placeholder {
    font-weight: normal;
    color: var(--text-secondary, #aaa);
  }

  .btn-remove {
    padding: 0.1rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: 0.25rem;
    background: transparent;
    cursor: pointer;
    font-size: 0.75rem;
    color: var(--text-secondary);
    line-height: 1;
  }

  .btn-remove:hover {
    background: var(--nav-item-hover);
    color: inherit;
  }

  .ssh-form {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .ssh-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .ssh-label {
    font-size: 0.78rem;
    font-weight: 600;
    white-space: nowrap;
    min-width: 2.75rem;
    color: var(--text-secondary);
  }

  .ssh-input {
    padding: 0.2rem 0.45rem;
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    background: var(--glass-card-bg);
    font-size: 0.78rem;
    color: inherit;
    min-width: 0;
  }

  .ssh-input--grow {
    flex: 1;
  }

  .ssh-input--port {
    width: 4.5rem;
  }

  .ssh-auth-toggle {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.78rem;
    cursor: pointer;
    user-select: none;
    margin-left: auto;
    white-space: nowrap;
  }

  .cmd-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .cmd-row button {
    padding: 0.2rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: var(--glass-card-bg);
    cursor: pointer;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .cmd-row button:hover {
    background: var(--nav-item-hover);
  }

  .cmd {
    flex: 1;
    font-size: 0.75rem;
    word-break: break-all;
    white-space: pre-wrap;
    font-family: monospace;
  }

  .note {
    font-size: 0.8rem;
    color: var(--text-secondary, #666);
  }

  .inline-code {
    font-family: monospace;
    font-size: 0.78rem;
    background: var(--glass-card-bg);
    border: 1px solid var(--border);
    border-radius: 0.2rem;
    padding: 0.05rem 0.3rem;
    white-space: nowrap;
  }
</style>
