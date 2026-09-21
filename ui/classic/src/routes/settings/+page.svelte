<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import ConnectorSettingsBlock from '$lib/components/connectors/ConnectorSettingsBlock.svelte';
  import LanguageSettings from '$lib/components/settings/LanguageSettings.svelte';
  import AppModeSettings from '$lib/components/settings/AppModeSettings.svelte';
  import ActiveUiSettings from '$lib/components/settings/ActiveUiSettings.svelte';
  import CronJobsSettings from '$lib/components/settings/CronJobsSettings.svelte';
  import AppVersionSettings from '$lib/components/settings/AppVersionSettings.svelte';
  import { presenterTheme, useWebPresenter } from '$lib/stores/presenter.js';
  import {
    sendWsCommand,
    getPresentationFontsStatus,
    installPresentationFonts,
    hostCapabilities,
    getSlideFolder,
    setSlideFolder,
    getSongSlideFolder,
    setSongSlideFolder,
  } from '@metocast/core-client';
  import type { PresentationFontsStatus } from '@metocast/core-client';

  let fontStatus = $state<PresentationFontsStatus | null>(null);
  let installingFonts = $state(false);
  let fontBookOpened = $state(false);
  let fontError = $state('');
  let songFolder = $state('');
  let bibleFolder = $state('');
  let folderSaving = $state<'song' | 'bible' | null>(null);
  let folderError = $state('');

  onMount(async () => {
    if (hostCapabilities.fonts) {
      fontStatus = await getPresentationFontsStatus().catch(() => null);
    }
    const [song, bible] = await Promise.all([
      getSongSlideFolder().catch(() => null),
      getSlideFolder().catch(() => null),
    ]);
    songFolder = song?.path ?? '';
    bibleFolder = bible?.path ?? '';
  });

  onMount(() => {
    const refreshFonts = async () => {
      if (!hostCapabilities.fonts) return;
      fontStatus = await getPresentationFontsStatus().catch(() => fontStatus);
      if (fontStatus?.installed) fontBookOpened = false;
    };
    window.addEventListener('focus', refreshFonts);
    return () => window.removeEventListener('focus', refreshFonts);
  });

  async function installFonts() {
    fontError = '';
    installingFonts = true;
    try {
      fontStatus = await installPresentationFonts();
      fontBookOpened = true;
    } catch (error) {
      fontError = error instanceof Error ? error.message : String(error);
    } finally {
      installingFonts = false;
    }
  }

  async function saveFolder(kind: 'song' | 'bible') {
    folderError = '';
    folderSaving = kind;
    try {
      if (kind === 'song') {
        songFolder = (await setSongSlideFolder(songFolder.trim())).path;
      } else {
        bibleFolder = (await setSlideFolder(bibleFolder.trim())).path;
      }
    } catch (error) {
      folderError = error instanceof Error ? error.message : String(error);
    } finally {
      folderSaving = null;
    }
  }

  function handleWebPresenterToggle(e: Event) {
    const enabled = (e.target as HTMLInputElement).checked;
    sendWsCommand('presentation.set_use_web_presenter', { enabled });
  }

  function handlePresenterTheme(e: Event) {
    sendWsCommand('presentation.set_presenter_theme', {
      theme: (e.target as HTMLSelectElement).value,
    });
  }
</script>

<div class="settings-container">
  <h1>{$_('appSettings.title')}</h1>

  <LanguageSettings />
  <AppModeSettings />
  <ActiveUiSettings />

  <h2 class="section-heading">{$_('appSettings.connectors.title')}</h2>
  <ConnectorSettingsBlock connectorId="obs" />
  <ConnectorSettingsBlock connectorId="youtube" />
  <ConnectorSettingsBlock connectorId="facebook" />
  <ConnectorSettingsBlock connectorId="vmix" />
  <ConnectorSettingsBlock connectorId="atem" />
  <ConnectorSettingsBlock connectorId="broadlink" />
  <ConnectorSettingsBlock connectorId="discord" />
  <ConnectorSettingsBlock connectorId="szentiras" />

  <h2 class="section-heading">Presentations</h2>
  <section>
    <p class="note">
      Use the built-in web presenter to parse and display slides in the browser instead of opening
      Keynote or PowerPoint. Only <code>.pptx</code> files are supported.
    </p>
    <label class="toggle-label">
      <input type="checkbox" checked={$useWebPresenter} onchange={handleWebPresenterToggle} />
      Use web presenter
    </label>
    <label class="theme-label">
      Presenter design
      <select value={$presenterTheme} onchange={handlePresenterTheme}>
        <option value="classic">Classic</option>
        <option value="editorial">Editorial</option>
      </select>
    </label>
    <div class="presentation-folders">
      <strong>{$_('appSettings.presentationFolders.title')}</strong>
      <form
        onsubmit={(event) => {
          event.preventDefault();
          void saveFolder('song');
        }}
      >
        <label for="song-slide-folder">{$_('appSettings.presentationFolders.song')}</label>
        <div class="folder-input">
          <input id="song-slide-folder" bind:value={songFolder} />
          <button type="submit" disabled={folderSaving !== null}>
            {folderSaving === 'song'
              ? $_('appSettings.presentationFolders.saving')
              : $_('appSettings.presentationFolders.save')}
          </button>
        </div>
      </form>
      <form
        onsubmit={(event) => {
          event.preventDefault();
          void saveFolder('bible');
        }}
      >
        <label for="bible-slide-folder">{$_('appSettings.presentationFolders.bible')}</label>
        <div class="folder-input">
          <input id="bible-slide-folder" bind:value={bibleFolder} />
          <button type="submit" disabled={folderSaving !== null}>
            {folderSaving === 'bible'
              ? $_('appSettings.presentationFolders.saving')
              : $_('appSettings.presentationFolders.save')}
          </button>
        </div>
      </form>
      {#if folderError}<p class="status-error">{folderError}</p>{/if}
    </div>
    {#if fontStatus?.supported}
      <div class="font-install">
        <strong>{$_('appSettings.presentationFonts.title')}</strong>
        <span>{$_('appSettings.presentationFonts.names')}</span>
        <small>{fontStatus.installDir}</small>
        {#if fontStatus.installed}
          <p class="status-ok">{$_('appSettings.presentationFonts.restart')}</p>
        {:else}
          <p>
            {fontBookOpened
              ? $_('appSettings.presentationFonts.fontBook')
              : $_('appSettings.presentationFonts.required')}
          </p>
          <button type="button" disabled={installingFonts} onclick={installFonts}>
            {installingFonts
              ? $_('appSettings.presentationFonts.installing')
              : $_('appSettings.presentationFonts.install')}
          </button>
        {/if}
        {#if fontError}<p class="status-error">{fontError}</p>{/if}
      </div>
    {/if}
  </section>

  <CronJobsSettings />
  <AppVersionSettings />
</div>

<style>
  .settings-container {
    max-width: 600px;
  }

  h1 {
    margin: 0 0 1.5rem;
    font-size: 1.5rem;
  }

  h2 {
    font-size: 1.125rem;
    margin: 0 0 0.75rem;
  }

  .section-heading {
    margin-top: 1.5rem;
  }

  section {
    padding: 1.25rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  .note {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0.5rem 0 1rem;
  }

  code {
    font-family: monospace;
    font-size: 0.875em;
    background: var(--content-bg);
    padding: 0.1em 0.3em;
    border-radius: 0.25rem;
  }

  .toggle-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .toggle-label input[type='checkbox'] {
    width: 1rem;
    height: 1rem;
    cursor: pointer;
  }

  .theme-label {
    display: grid;
    gap: 0.4rem;
    margin-top: 1rem;
    font-size: 0.875rem;
  }

  .theme-label select {
    min-height: 2.25rem;
    padding: 0 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--content-bg);
    color: var(--text-primary);
  }

  .font-install {
    display: grid;
    gap: 0.4rem;
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
  }

  .presentation-folders {
    display: grid;
    gap: 0.75rem;
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
  }

  .presentation-folders form,
  .folder-input {
    display: grid;
    gap: 0.4rem;
  }

  .presentation-folders label {
    font-size: 0.875rem;
    color: var(--text-secondary);
  }

  .folder-input {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .folder-input input {
    min-width: 0;
    padding: 0.5rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--content-bg);
    color: var(--text-primary);
  }

  .font-install span,
  .font-install small {
    color: var(--text-secondary);
  }

  .font-install button {
    justify-self: start;
  }

  .status-ok {
    margin: 0;
    color: var(--success, #16803a);
  }

  .status-error {
    margin: 0;
    color: var(--danger, #b42318);
  }
</style>
