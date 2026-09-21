<script lang="ts">
  import type { EventSummary } from '@metocast/core-client/schemas/event';

  interface Props {
    event: EventSummary;
  }

  let { event }: Props = $props();

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }
</script>

<a class="card" class:card--completed={event.isCompleted} href="/events/{event.id}">
  <div class="card__header">
    <h3>{event.title}</h3>
    <div class="card__badges">
      {#if event.isCompleted}
        <span class="badge badge--completed">Completed</span>
      {/if}
      <span class="badge"
        >{event.recordingCount} recording{event.recordingCount !== 1 ? 's' : ''}</span
      >
    </div>
  </div>
  <p class="card__meta">
    {#if event.speaker}
      <span>{event.speaker}</span> &middot;
    {/if}
    <span>{formatDate(event.dateTime)}</span>
  </p>
</a>

<style>
  .card {
    display: block;
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    text-decoration: none;
    color: inherit;
    transition: border-color 0.15s;
  }

  .card:hover {
    border-color: var(--accent);
  }

  .card--completed {
    opacity: 0.65;
  }

  .card__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .card__header h3 {
    margin: 0;
    font-size: 1rem;
  }

  .card__badges {
    display: flex;
    gap: 0.375rem;
    align-items: center;
  }

  .badge {
    font-size: 0.75rem;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    background: var(--accent-subtle);
    color: var(--accent);
    white-space: nowrap;
  }

  .badge--completed {
    background: var(--status-ok-bg);
    color: var(--status-ok-text);
  }

  .card__meta {
    margin: 0;
    font-size: 0.875rem;
    color: var(--text-secondary);
  }
</style>
