<script lang="ts">
  import { onMount } from 'svelte';
  import { toast } from 'svelte-sonner';
  import { _ } from 'svelte-i18n';
  import { serverUrl, localNetworkUrl } from '$lib/stores/server-url.js';
  import {
    captionSettingsStore,
    RESOLUTION_DIMENSIONS,
    getCaptionHeight,
    type CaptionSettings,
  } from '@metocast/core-client/utils/caption-store';

  let settings: CaptionSettings = $state(captionSettingsStore.getDefaultSettings());
  let isLoading = $state(true);
  let isSaving = $state(false);
  let urlCopied = $state(false);

  onMount(async () => {
    try {
      settings = await captionSettingsStore.getSettings();
    } catch (error) {
      console.error('Failed to load caption settings:', error);
      toast.error('Failed to load caption settings');
    } finally {
      isLoading = false;
    }
  });

  function getOBSDimensions(): { width: number; height: number } {
    const base = RESOLUTION_DIMENSIONS[settings.resolution];
    if (settings.type === 'caption') {
      return { width: base.width, height: getCaptionHeight('caption', settings.resolution) };
    }
    return { width: base.width, height: base.height };
  }

  const baseUrl = $derived($localNetworkUrl || $serverUrl);

  const captionUrl = $derived(() => {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- builds a query string inside a derived, never mutated after
    const params = new URLSearchParams();
    params.set('type', settings.type);
    params.set('resolution', settings.resolution);
    if (settings.title) params.set('title', settings.title);
    if (settings.boldText) params.set('bold', settings.boldText);
    if (settings.lightText) params.set('light', settings.lightText);
    params.set('color', settings.color);
    params.set('showLogo', settings.showLogo && settings.svgLogo ? 'true' : 'false');
    if (settings.logoAlt) params.set('alt', settings.logoAlt);
    return `${baseUrl}/caption?${params.toString()}`;
  });

  async function handleSave() {
    isSaving = true;
    try {
      await captionSettingsStore.saveSettings(settings);
      toast.success('Caption settings saved');
    } catch (error) {
      console.error('Failed to save caption settings:', error);
      toast.error('Failed to save caption settings');
    } finally {
      isSaving = false;
    }
  }

  async function handleReset() {
    settings = captionSettingsStore.getDefaultSettings();
    await handleSave();
  }

  async function handleCopyUrl() {
    await navigator.clipboard.writeText(captionUrl());
    urlCopied = true;
    toast.success('Caption URL copied to clipboard');
    setTimeout(() => {
      urlCopied = false;
    }, 2000);
  }

  const obsDims = $derived(getOBSDimensions());
  const previewScale = $derived(Math.min(1, 500 / obsDims.width));
  const scaledHeight = $derived(Math.round(obsDims.height * previewScale));
</script>

<svelte:head>
  <link
    href="https://fonts.googleapis.com/css2?family=Oswald:wght@300;600&display=swap"
    rel="stylesheet"
  />
</svelte:head>

<div class="page">
  <h1>OBS Caption</h1>
  <p class="subtitle">Configure the caption browser source for OBS Studio</p>

  {#if isLoading}
    <p class="loading">{$_('common.loading')}</p>
  {:else}
    <div class="layout">
      <!-- Left: settings -->
      <div class="glass-card settings-card">
        <h2>Settings</h2>

        <div class="field">
          <label for="resolution">Resolution</label>
          <select id="resolution" bind:value={settings.resolution}>
            <option value="1080p">1080p (1920×1080)</option>
            <option value="4k">4K (3840×2160)</option>
          </select>
        </div>

        <div class="field">
          <label for="caption-type">Caption Type</label>
          <select id="caption-type" bind:value={settings.type}>
            <option value="caption">Caption Bar</option>
            <option value="preview">Preview Full Screen</option>
          </select>
        </div>

        {#if settings.type === 'preview'}
          <div class="field">
            <label for="caption-title">Title</label>
            <input
              id="caption-title"
              type="text"
              bind:value={settings.title}
              placeholder="Service title"
            />
          </div>
        {/if}

        <div class="field">
          <label for="bold-text">Bold Text</label>
          <input
            id="bold-text"
            type="text"
            bind:value={settings.boldText}
            placeholder="e.g. John 3:16"
          />
        </div>

        <div class="field">
          <label for="light-text">Light Text</label>
          <input
            id="light-text"
            type="text"
            bind:value={settings.lightText}
            placeholder="e.g. Sunday Service"
          />
        </div>

        <div class="field">
          <label for="color">Color</label>
          <select id="color" bind:value={settings.color}>
            <option value="black">Black</option>
            <option value="red">Red</option>
          </select>
        </div>

        <div class="field checkbox-field">
          <label class="checkbox-label" for="show-logo">
            <input id="show-logo" type="checkbox" bind:checked={settings.showLogo} />
            Show Logo
          </label>
        </div>

        {#if settings.showLogo}
          <div class="field">
            <label for="logo-alt">Logo Alt Text</label>
            <input
              id="logo-alt"
              type="text"
              bind:value={settings.logoAlt}
              placeholder="e.g. Church name"
            />
          </div>
        {/if}

        <div class="field">
          <label for="svg-logo">SVG Logo</label>
          <textarea
            id="svg-logo"
            bind:value={settings.svgLogo}
            rows={4}
            placeholder="Paste SVG source here"
          ></textarea>
        </div>

        <div class="button-row">
          <button class="btn-primary" onclick={handleSave} disabled={isSaving}>
            {isSaving ? 'Saving…' : 'Save'}
          </button>
          <button class="btn-secondary" onclick={handleReset} disabled={isSaving}> Reset </button>
        </div>
      </div>

      <!-- Right: preview + OBS info -->
      <div class="right-col">
        <!-- Live preview -->
        <div class="glass-card preview-card">
          <h2>Live Preview</h2>
          <div class="preview-container" style="height: {scaledHeight}px;">
            {#key captionUrl()}
              <iframe
                title="Caption Preview"
                src={captionUrl()}
                width={obsDims.width}
                height={obsDims.height}
                style="transform: scale({previewScale}); transform-origin: 0 0; border: none;"
              ></iframe>
            {/key}
          </div>
        </div>

        <!-- OBS Browser Source info -->
        <div class="glass-card obs-card">
          <h2>OBS Browser Source</h2>
          <p class="note">Add a Browser Source in OBS and paste this URL:</p>
          <div class="url-block">
            <code>{captionUrl()}</code>
          </div>
          <div class="obs-info">
            <span class="dim-label">Width: <strong>{obsDims.width}</strong></span>
            <span class="dim-label">Height: <strong>{obsDims.height}</strong></span>
          </div>
          <button class="btn-primary" onclick={handleCopyUrl}>
            {urlCopied ? 'Copied!' : 'Copy URL'}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .page {
    max-width: 1100px;
  }

  h1 {
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 0.25rem;
  }

  .subtitle {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0 0 1.5rem;
  }

  .loading {
    color: var(--text-secondary);
    font-size: 0.875rem;
  }

  .layout {
    display: grid;
    grid-template-columns: 320px 1fr;
    gap: 1.25rem;
    align-items: start;
  }

  .settings-card,
  .preview-card,
  .obs-card {
    padding: 1.25rem;
  }

  .right-col {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  h2 {
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 1rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin-bottom: 0.75rem;
  }

  .field label,
  .checkbox-label {
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-primary);
  }

  .checkbox-field {
    margin-bottom: 0.75rem;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  input[type='text'],
  select,
  textarea {
    padding: 0.375rem 0.625rem;
    border: 1px solid var(--input-border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    background: var(--input-bg);
    color: var(--text-primary);
    width: 100%;
    box-sizing: border-box;
  }

  input[type='text']:focus,
  select:focus,
  textarea:focus {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-color: var(--accent);
  }

  textarea {
    resize: vertical;
    font-family: monospace;
    font-size: 0.75rem;
  }

  .button-row {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }

  .preview-container {
    width: 100%;
    overflow: hidden;
    border-radius: 6px;
    border: 1px solid var(--glass-border);
  }

  .note {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0 0 0.75rem;
  }

  .url-block {
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 0.375rem;
    padding: 0.5rem 0.75rem;
    margin-bottom: 0.75rem;
    overflow-x: auto;
  }

  .url-block code {
    font-size: 0.75rem;
    color: var(--text-primary);
    word-break: break-all;
  }

  .obs-info {
    display: flex;
    gap: 1.5rem;
    margin-bottom: 0.75rem;
  }

  .dim-label {
    font-size: 0.875rem;
    color: var(--text-secondary);
  }

  .btn-primary {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: #fff;
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
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--accent-subtle);
  }

  .btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
