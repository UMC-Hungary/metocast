<script lang="ts">
  import { assignUntrackedRecording } from '@metocast/core-client';
  import { listEvents } from '@metocast/core-client';
  import type { EventSummary } from '@metocast/core-client/schemas/event';
  import { onMount } from 'svelte';

  interface Props {
    untrackedId: string;
    onclose: () => void;
  }

  let { untrackedId, onclose }: Props = $props();

  let eventList = $state<EventSummary[]>([]);
  let selectedEventId = $state('');
  let loading = $state(false);
  let assigning = $state(false);
  let error = $state('');

  onMount(async () => {
    loading = true;
    try {
      eventList = await listEvents();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  async function assign() {
    if (!selectedEventId) return;
    assigning = true;
    error = '';
    try {
      await assignUntrackedRecording(untrackedId, selectedEventId);
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      assigning = false;
    }
  }
</script>

<div
  class="overlay"
  role="presentation"
  onkeydown={(e) => e.key === 'Escape' && onclose()}
  onclick={onclose}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Assign recording to event"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <h3>Assign Recording to Event</h3>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if loading}
      <p class="hint">Loading events…</p>
    {:else}
      <label for="event-select" class="label">Select event</label>
      <select id="event-select" bind:value={selectedEventId} class="select">
        <option value="">— choose an event —</option>
        {#each eventList as ev (ev.id)}
          <option value={ev.id}>{ev.title}</option>
        {/each}
      </select>
    {/if}

    <div class="actions">
      <button class="btn-cancel" onclick={onclose}>Cancel</button>
      <button class="btn-assign" onclick={assign} disabled={!selectedEventId || assigning}>
        {assigning ? 'Assigning…' : 'Assign'}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--modal-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }

  .dialog {
    background: var(--modal-card-bg);
    border-radius: 0.75rem;
    padding: 1.5rem;
    width: 100%;
    max-width: 28rem;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.12);
  }

  .dialog h3 {
    margin: 0 0 1rem;
    font-size: 1.125rem;
  }

  .label {
    display: block;
    font-size: 0.875rem;
    font-weight: 500;
    margin-bottom: 0.375rem;
    color: var(--text-primary);
  }

  .select {
    width: 100%;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    margin-bottom: 1rem;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.75rem;
  }

  .btn-cancel {
    padding: 0.5rem 1rem;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-cancel:hover {
    background: var(--content-bg);
  }

  .btn-assign {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-assign:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-assign:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .error {
    padding: 0.5rem 0.75rem;
    background: var(--status-err-bg);
    color: var(--status-err-text);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    margin-bottom: 1rem;
  }

  .hint {
    color: var(--text-secondary);
    font-size: 0.875rem;
    margin-bottom: 1rem;
  }
</style>
