<script lang="ts">
  import Button from '../primitives/Button.svelte';
  import Dot from '../primitives/Dot.svelte';

  interface Props {
    title: string;
    /** What went wrong, in prose. The reference sets this in the serif italic it uses for prose. */
    body?: string;
    retryLabel?: string;
    onretry?: () => void;
    /** Announces the failure when it appears in place of content the user was waiting for. */
    announce?: boolean;
  }

  let { title, body = '', retryLabel = 'Try again', onretry, announce = true }: Props = $props();
</script>

<div class="error" role={announce ? 'alert' : undefined}>
  <p class="title"><Dot color="var(--status-error)" size={6} />{title}</p>
  {#if body}<p class="body">{body}</p>{/if}
  {#if onretry}
    <div class="action"><Button variant="secondary" onclick={onretry}>{retryLabel}</Button></div>
  {/if}
</div>

<style>
  .error {
    padding: var(--c-error-state-padding-block) var(--ui-gutter);
    border-left: var(--ui-border-emphasis) solid var(--status-error);
    background: var(--surface-sunken);
  }

  .title {
    display: flex;
    align-items: center;
    gap: var(--c-status-dot-gap);
    margin: 0;
    font-family: var(--type-label-xs-family);
    font-size: var(--type-label-xs-size);
    letter-spacing: var(--type-label-xs-track);
    text-transform: var(--type-label-xs-transform);
    color: var(--status-error);
  }

  .body {
    margin: var(--c-error-state-gap) 0 0;
    font-family: var(--type-quote-family);
    font-style: italic;
    font-size: var(--type-quote-size);
    line-height: var(--type-quote-leading);
    color: var(--text-secondary);
  }

  .action {
    margin-top: var(--c-error-state-gap);
  }
</style>
