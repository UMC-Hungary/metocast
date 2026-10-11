<script lang="ts">
  import { page } from '$app/stores';
  import EventEditor from '$lib/EventEditor.svelte';
  import OrderOfService from '$lib/OrderOfService.svelte';

  const eventId = $derived($page.params.id ?? '');
</script>

<EventEditor {eventId} />
{#if eventId}
  <div class="order">
    <OrderOfService {eventId} mode="edit" />
  </div>
{/if}

<style>
  .order {
    max-width: 620px;
    margin: 0 auto;
    padding: 0 24px 48px;
  }
</style>
