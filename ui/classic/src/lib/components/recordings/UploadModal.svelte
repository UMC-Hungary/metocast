<script lang="ts">
  import { untrack } from 'svelte';
  import { flagRecordingsForUpload } from '@metocast/core-client';
  import { triggerUploadCycle } from '@metocast/core-client';
  import type { Recording } from '@metocast/core-client/schemas/recording';
  import type { Event } from '@metocast/core-client/schemas/event';
  import { youtubeStatus, facebookStatus } from '$lib/stores/connectors.js';

  interface Props {
    event: Event;
    recordings: Recording[];
    onclose: () => void;
    onuploaded: (recordings: Recording[]) => void;
  }

  let { event, recordings, onclose, onuploaded }: Props = $props();

  const ytConnected = $derived($youtubeStatus === 'connected');
  const fbConnected = $derived($facebookStatus === 'connected');

  let selectedIds = $state<Set<string>>(
    untrack(() => {
      const firstId = recordings.length === 1 ? recordings[0]?.id : undefined;
      return firstId ? new Set([firstId]) : new Set();
    }),
  );
  let customTitles = $state<Record<string, string>>(
    untrack(() => Object.fromEntries(recordings.map((r) => [r.id, r.customTitle ?? event.title]))),
  );
  let customDescriptions = $state<Record<string, string>>(
    untrack(() => Object.fromEntries(recordings.map((r) => [r.id, r.customDescription ?? '']))),
  );
  let youtubeVisibility = $state<'private' | 'unlisted' | 'public'>(
    untrack(
      () =>
        (event.connections.find((c) => c.platform === 'youtube')?.privacyStatus as
          | 'private'
          | 'unlisted'
          | 'public'
          | undefined) ?? 'private',
    ),
  );
  let facebookVisibility = $state<'ONLY_ME' | 'FRIENDS' | 'EVERYONE'>(
    untrack(
      () =>
        (event.connections.find((c) => c.platform === 'facebook')?.privacyStatus as
          | 'ONLY_ME'
          | 'FRIENDS'
          | 'EVERYONE'
          | undefined) ?? 'ONLY_ME',
    ),
  );
  let saving = $state(false);
  let error = $state('');

  function toggleSelection(id: string) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- copy-then-assign; reactivity comes from the $state assignment
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selectedIds = next;
  }

  async function startUpload() {
    if (selectedIds.size === 0) return;

    const platforms: string[] = [];
    if (ytConnected) platforms.push('youtube');
    if (fbConnected) platforms.push('facebook');
    if (platforms.length === 0) {
      error = 'No platforms connected. Connect YouTube or Facebook first.';
      return;
    }

    saving = true;
    error = '';
    try {
      const items = [...selectedIds].map((id) => ({
        recording_id: id,
        custom_title: customTitles[id] || undefined,
        custom_description: customDescriptions[id] || undefined,
        youtube_visibility: ytConnected ? youtubeVisibility : undefined,
        facebook_visibility: fbConnected ? facebookVisibility : undefined,
        platforms,
      }));

      await flagRecordingsForUpload(event.id, items);
      await triggerUploadCycle();

      const updated = recordings.map((r) =>
        selectedIds.has(r.id) ? { ...r, uploadable: true } : r,
      );
      onuploaded(updated);
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-label="Upload Recordings">
  <div class="modal">
    <div class="modal__header">
      <h2>Upload Recordings</h2>
      <button class="modal__close" onclick={onclose} aria-label="Close">✕</button>
    </div>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if recordings.length === 0}
      <p class="empty">No recordings to upload.</p>
    {:else}
      <ul class="recording-list">
        {#each recordings as rec (rec.id)}
          <li class="recording-item">
            <label class="recording-item__check">
              <input
                type="checkbox"
                checked={selectedIds.has(rec.id)}
                onchange={() => toggleSelection(rec.id)}
              />
              <span class="recording-item__name">{rec.customTitle ?? rec.fileName}</span>
            </label>
            {#if selectedIds.has(rec.id)}
              <div class="recording-item__fields">
                <label class="field">
                  <span>Title</span>
                  <input type="text" bind:value={customTitles[rec.id]} placeholder={rec.fileName} />
                </label>
                <label class="field">
                  <span>Description</span>
                  <textarea bind:value={customDescriptions[rec.id]} rows={2}></textarea>
                </label>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if ytConnected || fbConnected}
      <div class="visibility-section">
        <h3>Visibility</h3>
        {#if ytConnected}
          <label class="field field--row">
            <span>YouTube</span>
            <select bind:value={youtubeVisibility}>
              <option value="private">Private</option>
              <option value="unlisted">Unlisted</option>
              <option value="public">Public</option>
            </select>
          </label>
        {/if}
        {#if fbConnected}
          <label class="field field--row">
            <span>Facebook</span>
            <select bind:value={facebookVisibility}>
              <option value="ONLY_ME">Only Me</option>
              <option value="FRIENDS">Friends</option>
              <option value="EVERYONE">Everyone</option>
            </select>
          </label>
        {/if}
      </div>
    {:else}
      <p class="warn">No upload platforms connected. Connect YouTube or Facebook in Settings.</p>
    {/if}

    <div class="modal__actions">
      <button
        class="btn-primary"
        onclick={startUpload}
        disabled={saving || selectedIds.size === 0 || (!ytConnected && !fbConnected)}
      >
        {saving ? 'Starting…' : 'Start Upload'}
      </button>
      <button class="btn-secondary" onclick={onclose} disabled={saving}>Skip</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--modal-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .modal {
    background: var(--modal-card-bg);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    padding: 1.5rem;
    width: min(560px, 90vw);
    max-height: 80vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .modal__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .modal__header h2 {
    margin: 0;
    font-size: 1.125rem;
  }

  .modal__close {
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 1rem;
    cursor: pointer;
    padding: 0.25rem;
    line-height: 1;
  }

  .modal__close:hover {
    color: var(--text-primary);
  }

  .recording-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .recording-item {
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    padding: 0.625rem 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .recording-item__check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    font-size: 0.875rem;
    color: var(--text-primary);
  }

  .recording-item__name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .recording-item__fields {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    padding-left: 1.5rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.8125rem;
    color: var(--text-secondary);
  }

  .field--row {
    flex-direction: row;
    align-items: center;
    gap: 0.75rem;
  }

  .field--row span {
    min-width: 5rem;
  }

  .field input,
  .field textarea,
  .field select {
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--input-border);
    border-radius: 0.25rem;
    font-size: 0.8125rem;
    background: var(--input-bg, transparent);
    color: var(--text-primary);
    width: 100%;
    box-sizing: border-box;
  }

  .field--row select {
    flex: 1;
  }

  .field textarea {
    resize: vertical;
  }

  .visibility-section {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
  }

  .visibility-section h3 {
    margin: 0 0 0.25rem;
    font-size: 0.875rem;
    font-weight: 600;
  }

  .modal__actions {
    display: flex;
    gap: 0.75rem;
    justify-content: flex-end;
  }

  .btn-primary {
    padding: 0.5rem 1.25rem;
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
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-secondary {
    padding: 0.5rem 1.25rem;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-secondary:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--nav-item-hover);
  }

  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .error {
    padding: 0.625rem 0.75rem;
    background: var(--status-err-bg);
    color: var(--status-err-text);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    margin: 0;
  }

  .warn {
    font-size: 0.875rem;
    color: var(--status-warn-text);
    background: var(--status-warn-bg);
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    margin: 0;
  }

  .empty {
    color: var(--text-secondary);
    font-size: 0.875rem;
  }
</style>
