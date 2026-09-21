<script lang="ts">
  import { page } from '$app/stores';
  import EventEditor from '$lib/EventEditor.svelte';
</script>

<EventEditor eventId={$page.params.id} />
