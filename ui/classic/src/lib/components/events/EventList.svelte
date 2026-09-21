<script lang="ts">
  import type { EventSummary } from '@metocast/core-client/schemas/event';
  import EventCard from './EventCard.svelte';

  interface Props {
    events: EventSummary[];
    loading: boolean;
  }

  let { events, loading }: Props = $props();
</script>

{#if loading}
  <p>Loading events…</p>
{:else if events.length === 0}
  <p class="empty">No events yet. <a href="/events/new">Create one</a>.</p>
{:else}
  <ul class="list">
    {#each events as event (event.id)}
      <li>
        <EventCard {event} />
      </li>
    {/each}
  </ul>
{/if}

<style>
  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .empty {
    color: var(--text-secondary);
  }
</style>
