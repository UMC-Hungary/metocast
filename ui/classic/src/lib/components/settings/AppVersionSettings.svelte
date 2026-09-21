<script lang="ts">
  import * as host from '@metocast/core-client';
  import { hostCapabilities } from '@metocast/core-client';
  import { toast } from 'svelte-sonner';
  import { _ } from 'svelte-i18n';
  import { updaterStore } from '$lib/stores/updater.js';

  async function checkForUpdates() {
    updaterStore.update((s) => ({ ...s, status: 'checking', error: null }));
    try {
      const info = await host.checkForUpdates();
      if (info) {
        updaterStore.set({
          status: 'available',
          info,
          error: null,
          lastChecked: new Date(),
        });
      } else {
        updaterStore.update((s) => ({
          ...s,
          status: 'up-to-date',
          lastChecked: new Date(),
        }));
      }
    } catch (e) {
      updaterStore.update((s) => ({
        ...s,
        status: 'error',
        error: e instanceof Error ? e.message : String(e),
        lastChecked: new Date(),
      }));
    }
  }

  async function installUpdate() {
    updaterStore.update((s) => ({ ...s, status: 'installing', error: null }));
    try {
      await host.installUpdate();
      updaterStore.update((s) => ({ ...s, status: 'installed' }));
      toast.success($_('appSettings.updater.installed'));
    } catch (e) {
      updaterStore.update((s) => ({
        ...s,
        status: 'error',
        error: e instanceof Error ? e.message : String(e),
      }));
      toast.error($_('appSettings.updater.installFailed'));
    }
  }
</script>

{#if hostCapabilities.updater}
  <section>
    <h2>{$_('appSettings.updater.title')}</h2>

    {#if $updaterStore.info?.currentVersion}
      <p class="version-row">
        <span class="label">{$_('appSettings.updater.currentVersion')}</span>
        <span class="value">v{$updaterStore.info.currentVersion}</span>
      </p>
    {/if}

    {#if $updaterStore.status === 'available' && $updaterStore.info}
      <div class="banner banner-update">
        {$_('appSettings.updater.updateAvailable', {
          values: { version: $updaterStore.info.latestVersion },
        })}
        <button class="action-btn" onclick={installUpdate}>
          {$_('appSettings.updater.install')}
        </button>
      </div>
    {:else if $updaterStore.status === 'installing'}
      <p class="status-ok">{$_('appSettings.updater.installing')}</p>
    {:else if $updaterStore.status === 'installed'}
      <p class="status-ok">{$_('appSettings.updater.installed')}</p>
    {:else if $updaterStore.status === 'up-to-date'}
      <p class="status-ok">{$_('appSettings.updater.upToDate')}</p>
    {:else if $updaterStore.status === 'error'}
      <p class="status-error">
        {$_('appSettings.updater.errorChecking')}
        {#if $updaterStore.error}
          — {$updaterStore.error}
        {/if}
      </p>
    {/if}

    {#if $updaterStore.lastChecked}
      <p class="last-checked">
        {$_('appSettings.updater.lastChecked')}: {$updaterStore.lastChecked.toLocaleTimeString()}
      </p>
    {/if}

    <button
      class="check-btn"
      disabled={$updaterStore.status === 'checking' || $updaterStore.status === 'installing'}
      onclick={checkForUpdates}
    >
      {$updaterStore.status === 'checking'
        ? $_('appSettings.updater.checking')
        : $_('appSettings.updater.checkForUpdates')}
    </button>
  </section>
{/if}

<style>
  section {
    padding: 1.25rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  h2 {
    font-size: 1.125rem;
    margin: 0 0 0.75rem;
  }

  .version-row {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    margin: 0 0 0.75rem;
    font-size: 0.875rem;
  }

  .label {
    color: var(--text-secondary);
  }

  .value {
    font-weight: 600;
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.625rem 0.875rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    margin-bottom: 0.75rem;
  }

  .banner-update {
    background: oklch(0.96 0.02 250);
    color: oklch(0.3 0.1 250);
    border: 1px solid oklch(0.8 0.05 250);
  }

  .action-btn {
    margin-left: auto;
    padding: 0.25rem 0.625rem;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 0.25rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .action-btn:hover {
    opacity: 0.9;
  }

  .status-ok {
    font-size: 0.875rem;
    color: oklch(0.45 0.15 145);
    margin: 0 0 0.75rem;
  }

  .status-error {
    font-size: 0.875rem;
    color: oklch(0.45 0.2 25);
    margin: 0 0 0.75rem;
  }

  .last-checked {
    font-size: 0.75rem;
    color: var(--text-secondary);
    margin: 0 0 0.75rem;
  }

  .check-btn {
    padding: 0.5rem 1rem;
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .check-btn:hover:not(:disabled) {
    background: var(--nav-item-hover);
  }

  .check-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
