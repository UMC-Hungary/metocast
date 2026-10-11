<script lang="ts">
  import { resolve } from '$app/paths';
  import { _ } from 'svelte-i18n';
  import { live } from '$lib/live.svelte';

  const credit = $derived(live.transcriptCredit);
  // OpenAI refusing for quota outranks whatever the computed balance says.
  const noCredit = $derived(live.transcription?.reason === 'noCredit');
  const updated = $derived(
    credit
      ? new Date(credit.asOf).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
      : '',
  );
</script>

{#if noCredit}
  <p class="credit low">{$_('transcript.noCredit')}</p>
{:else if credit}
  <p class="credit" class:low={credit.low}>
    {$_('transcript.credit', {
      values: { amount: `$${credit.balanceUsd.toFixed(2)}`, time: updated },
    })}
  </p>
{:else}
  <p class="credit">
    <a href={resolve('/settings/connectors/openai')}>{$_('transcript.addAdminKey')}</a>
  </p>
{/if}

<style>
  .credit {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
  }
  .credit a {
    color: inherit;
  }
  .low {
    color: var(--status-error);
  }
</style>
