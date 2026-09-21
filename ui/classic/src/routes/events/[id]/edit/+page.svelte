<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { getEvent } from '@metocast/core-client';
  import type { Event } from '@metocast/core-client/schemas/event';
  import CreateEventForm from '$lib/components/events/CreateEventForm.svelte';

  const id = page.params.id ?? '';

  let event = $state<Event | null>(null);
  let loading = $state(true);
  let error = $state('');

  onMount(async () => {
    try {
      event = await getEvent(id);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  });

  function handleUpdated(updated: Event) {
    goto(`/events/${updated.id}`);
  }
</script>

<svelte:head>
  <title>{event ? `Edit: ${event.title}` : 'Edit Event'} — Metocast</title>
</svelte:head>

<h1>Edit Event</h1>

{#if error}
  <p class="error" role="alert">{error}</p>
{:else if loading}
  <p>Loading…</p>
{:else if event}
  <CreateEventForm initialEvent={event} onupdated={handleUpdated} />
{/if}

<style>
  h1 {
    margin-bottom: 1.25rem;
    font-size: 1.5rem;
  }
  .error {
    padding: 0.75rem;
    background: var(--status-err-bg);
    color: var(--status-err-text);
    border-radius: 0.375rem;
  }
</style>
