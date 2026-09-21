<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getAppMode, resetSetup, hostCapabilities } from '@metocast/core-client';
  import { appMode } from '$lib/stores/mode.js';
  import type { AppMode } from '$lib/stores/mode.js';
  import { _ } from 'svelte-i18n';

  let currentMode: AppMode | null = $state(null);
  let resetting = $state(false);
  let errorMessage = $state('');

  onMount(async () => {
    try {
      currentMode = (await getAppMode()) as AppMode | null;
    } catch (e) {
      console.error('Settings load error:', e);
    }
  });

  async function changeMode() {
    resetting = true;
    errorMessage = '';
    try {
      await resetSetup();
      appMode.set('server');
      await goto('/setup');
    } catch (e) {
      errorMessage = String(e);
      resetting = false;
    }
  }
</script>

{#if hostCapabilities.mode}
  <section>
    <h2>{$_('appSettings.appMode.title')}</h2>
    <p>
      {$_('appSettings.appMode.current')}: <strong>{currentMode ?? '—'}</strong>
    </p>
    <p class="note">
      {$_('appSettings.appMode.note')}
    </p>

    {#if errorMessage}
      <p class="error" role="alert">{errorMessage}</p>
    {/if}

    <button onclick={changeMode} disabled={resetting}>
      {resetting ? $_('appSettings.appMode.changing') : $_('appSettings.appMode.changeMode')}
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

  .note {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0.5rem 0 1rem;
  }

  .error {
    color: var(--status-err-text);
    font-size: 0.875rem;
    margin: 0 0 0.75rem;
  }
</style>
