<script lang="ts">
  import * as host from '@metocast/core-client';
  import { authToken } from '$lib/stores/server-url.js';

  let revealed = $state(false);
  let copied = $state(false);
  let refreshing = $state(false);
  let error = $state('');

  function maskedToken(token: string): string {
    if (token.length <= 8) return '••••••••';
    return token.slice(0, 4) + '••••••••••••••••••••••••••••' + token.slice(-4);
  }

  async function copyToken() {
    try {
      await navigator.clipboard.writeText($authToken);
      copied = true;
      setTimeout(() => {
        copied = false;
      }, 2000);
    } catch {
      error = 'Failed to copy to clipboard';
    }
  }

  async function refreshToken() {
    refreshing = true;
    error = '';
    try {
      const newToken = await host.refreshToken();
      authToken.set(newToken);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      refreshing = false;
    }
  }
</script>

<div class="token-display">
  <h3>Auth Token</h3>

  {#if error}
    <p class="token-display__error" role="alert">{error}</p>
  {/if}

  <div class="token-display__value">
    <code>{revealed ? $authToken : maskedToken($authToken)}</code>
  </div>

  <div class="token-display__actions">
    <button
      onclick={() => {
        revealed = !revealed;
      }}
    >
      {revealed ? 'Hide' : 'Reveal'}
    </button>
    <button onclick={copyToken}>
      {copied ? 'Copied!' : 'Copy'}
    </button>
    <button onclick={refreshToken} disabled={refreshing}>
      {refreshing ? 'Refreshing…' : 'Refresh Token'}
    </button>
  </div>

  <p class="token-display__warning">
    Refreshing the token will disconnect existing WebSocket clients.
  </p>
</div>

<style>
  .token-display {
    padding: 1.5rem;
    background: var(--content-bg);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    max-width: 600px;
  }

  .token-display h3 {
    margin: 0 0 1rem;
  }

  .token-display__value {
    padding: 0.75rem;
    background: var(--glass-card-bg);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    margin-bottom: 0.75rem;
    word-break: break-all;
  }

  .token-display__actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .token-display__actions button {
    padding: 0.5rem 1rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: var(--glass-card-bg);
    cursor: pointer;
    font-size: 0.875rem;
  }

  .token-display__actions button:hover {
    background: var(--nav-item-hover);
  }
  .token-display__actions button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .token-display__warning {
    margin: 0.75rem 0 0;
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .token-display__error {
    padding: 0.5rem;
    background: var(--status-err-bg);
    color: var(--status-err-text);
    border-radius: 0.25rem;
    font-size: 0.875rem;
    margin-bottom: 0.75rem;
  }
</style>
