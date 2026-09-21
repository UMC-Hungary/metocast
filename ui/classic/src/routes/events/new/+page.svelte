<script lang="ts">
  import { goto } from '$app/navigation';
  import CreateEventForm from '$lib/components/events/CreateEventForm.svelte';
  import type { Event } from '@metocast/core-client/schemas/event';

  function handleCreated(event: Event) {
    goto(`/events/${event.id}`);
  }
</script>

<svelte:head>
  <title>New Event — Metocast</title>
</svelte:head>

<h1>Create Event</h1>
<CreateEventForm oncreated={handleCreated} />

<style>
  h1 {
    margin-bottom: 1.25rem;
    font-size: 1.5rem;
  }
</style>
