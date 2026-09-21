<script lang="ts">
  import { youtubeStatus, facebookStatus } from '$lib/stores/connectors.js';
  import ReLoginModal from '$lib/components/ReLoginModal.svelte';

  let reLoginConnector = $state<'youtube' | 'facebook' | null>(null);
  let dismissedErrors = $state(new Set<string>());

  $effect(() => {
    if ($youtubeStatus === 'error' && !dismissedErrors.has('youtube')) {
      reLoginConnector = 'youtube';
    } else if ($facebookStatus === 'error' && !dismissedErrors.has('facebook')) {
      reLoginConnector = 'facebook';
    } else if (
      reLoginConnector !== null &&
      $youtubeStatus !== 'error' &&
      $facebookStatus !== 'error'
    ) {
      reLoginConnector = null;
    }
  });

  function dismiss() {
    if (reLoginConnector) {
      dismissedErrors = new Set([...dismissedErrors, reLoginConnector]);
    }
    reLoginConnector = null;
  }
</script>

{#if reLoginConnector !== null}
  <ReLoginModal connector={reLoginConnector} onclose={dismiss} />
{/if}
