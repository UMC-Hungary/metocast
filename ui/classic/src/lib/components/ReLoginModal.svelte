<script lang="ts">
  import { openExternal } from '@metocast/core-client';
  import { youtubeAuthUrl, facebookAuthUrl } from '@metocast/core-client';

  interface Props {
    connector: 'youtube' | 'facebook';
    onclose: () => void;
  }

  let { connector, onclose }: Props = $props();

  const names: Record<'youtube' | 'facebook', string> = {
    youtube: 'YouTube',
    facebook: 'Facebook',
  };

  let opening = $state(false);
  let error = $state('');

  async function reLogin() {
    opening = true;
    error = '';
    try {
      const url = connector === 'youtube' ? await youtubeAuthUrl() : await facebookAuthUrl();
      await openExternal(url);
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      opening = false;
    }
  }
</script>

<!-- Overlay -->
<div
  class="overlay"
  role="presentation"
  onclick={onclose}
  onkeydown={(e) => {
    if (e.key === 'Escape') onclose();
  }}
></div>

<!-- Dialog -->
<div
  class="modal"
  role="alertdialog"
  aria-modal="true"
  aria-labelledby="relogin-title"
  aria-describedby="relogin-desc"
>
  <h2 id="relogin-title" class="modal__title">Session Expired</h2>
  <p id="relogin-desc" class="modal__body">
    Your {names[connector]} session has expired. Please re-login to continue automatic event scheduling.
  </p>

  {#if error}
    <p class="modal__error" role="alert">{error}</p>
  {/if}

  <div class="modal__actions">
    <button class="btn-primary" onclick={reLogin} disabled={opening}>
      {opening ? 'Opening…' : `Re-login with ${names[connector]}`}
    </button>
    <button class="btn-secondary" onclick={onclose}>Dismiss</button>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--modal-backdrop);
    z-index: 100;
  }

  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 101;
    background: var(--modal-card-bg);
    border-radius: 0.75rem;
    padding: 2rem;
    max-width: 400px;
    width: calc(100% - 2rem);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.15);
  }

  .modal__title {
    font-size: 1.125rem;
    font-weight: 600;
    margin: 0 0 0.75rem;
    color: var(--text-primary);
  }

  .modal__body {
    font-size: 0.9rem;
    color: var(--text-secondary);
    margin: 0 0 1.25rem;
    line-height: 1.5;
  }

  .modal__error {
    color: var(--status-err-text);
    font-size: 0.875rem;
    margin: 0 0 0.75rem;
  }

  .modal__actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .btn-primary {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-secondary {
    padding: 0.5rem 1rem;
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-secondary:hover {
    background: var(--content-bg);
  }
</style>
