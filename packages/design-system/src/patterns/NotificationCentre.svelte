<script lang="ts">
  import type { Snippet } from 'svelte';
  import Button from '../primitives/Button.svelte';
  import EmptyState from './EmptyState.svelte';
  import Sheet from './Sheet.svelte';

  interface Props {
    open?: boolean;
    /** "3 items" or "All clear", already interpolated by the caller. */
    title: string;
    eyebrow?: string;
    /** True when there is nothing waiting, so the empty state shows instead of the list. */
    empty?: boolean;
    emptyTitle: string;
    emptyHint?: string;
    clearLabel?: string;
    onclear?: () => void;
    onclose?: () => void;
    children?: Snippet;
  }

  let {
    open = $bindable(false),
    title,
    eyebrow = 'Notifications',
    empty = false,
    emptyTitle,
    emptyHint = '',
    clearLabel = 'Clear all',
    onclear,
    onclose,
    children,
  }: Props = $props();
</script>

<Sheet bind:open {title} {eyebrow} ariaLabel={eyebrow} maxHeight="88%" {onclose}>
  {#snippet action()}
    {#if !empty && onclear}
      <Button variant="quiet" onclick={onclear}>{clearLabel}</Button>
    {/if}
  {/snippet}

  {#if empty}
    <EmptyState title={emptyTitle} hint={emptyHint} />
  {:else}
    <div class="list">{@render children?.()}</div>
  {/if}
</Sheet>

<style>
  .list {
    padding: var(--c-notification-centre-padding);
    display: flex;
    flex-direction: column;
    gap: var(--c-notification-centre-gap);
  }
</style>
