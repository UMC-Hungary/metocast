<script lang="ts">
  import { onMount } from 'svelte';
  import { listEvents } from '@metocast/core-client';
  import { listUntrackedRecordings, deleteUntrackedRecording } from '@metocast/core-client';
  import { events, eventsLoading, untrackedRecordings } from '$lib/stores/events.js';
  import { lastWsMessage } from '$lib/stores/ws.js';
  import EventList from '$lib/components/events/EventList.svelte';
  import RecordingList from '$lib/components/recordings/RecordingList.svelte';
  import AssignRecordingDialog from '$lib/components/recordings/AssignRecordingDialog.svelte';
  import RecordingsBlock from '$lib/components/recordings/RecordingsBlock.svelte';

  let assigningId = $state<string | null>(null);

  onMount(async () => {
    eventsLoading.set(true);
    try {
      const [data, untracked] = await Promise.all([listEvents(), listUntrackedRecordings()]);
      events.set(data);
      untrackedRecordings.set(untracked);
    } finally {
      eventsLoading.set(false);
    }
  });

  $effect(() => {
    const msg = $lastWsMessage;
    if (msg?.type === 'recording.detected' && msg.eventTitle === null) {
      // Reload untracked list when an untracked recording arrives
      listUntrackedRecordings().then((data) => untrackedRecordings.set(data));
    }
  });

  async function handleDelete(id: string, deleteFile: boolean) {
    await deleteUntrackedRecording(id, deleteFile);
    untrackedRecordings.update((list) => list.filter((r) => r.id !== id));
  }
</script>

<svelte:head>
  <title>Events — Metocast</title>
</svelte:head>

<div class="page-header">
  <h1>Events</h1>
  <a href="/events/new" class="btn">+ New Event</a>
</div>

<EventList events={$events} loading={$eventsLoading} />

<section class="recordings-section">
  <h2>Recordings</h2>
  <RecordingsBlock />
</section>

{#if $untrackedRecordings.length > 0}
  <section class="untracked">
    <h2>Untracked Recordings</h2>
    <p class="hint">These recordings were captured by OBS but could not be matched to an event.</p>
    <RecordingList
      recordings={$untrackedRecordings}
      onassign={(id) => (assigningId = id)}
      ondelete={handleDelete}
    />
  </section>
{/if}

{#if assigningId !== null}
  <AssignRecordingDialog untrackedId={assigningId} onclose={() => (assigningId = null)} />
{/if}

<style>
  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1.5rem;
  }

  .page-header h1 {
    margin: 0;
  }

  .btn {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: white;
    text-decoration: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
  }

  .btn:hover {
    filter: brightness(0.9);
  }

  .recordings-section {
    margin-top: 2.5rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--border);
  }

  .recordings-section h2 {
    margin: 0 0 1rem;
    font-size: 1.125rem;
  }

  .untracked {
    margin-top: 2.5rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--border);
  }

  .untracked h2 {
    margin: 0 0 0.5rem;
    font-size: 1.125rem;
  }

  .hint {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0 0 1rem;
  }
</style>
