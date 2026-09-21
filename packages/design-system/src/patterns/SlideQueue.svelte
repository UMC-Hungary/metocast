<script lang="ts" module>
  export interface QueueSlot {
    index: number;
    /** Empty when the slot is unassigned. */
    title?: string;
    /** The slot currently on screen. */
    loaded?: boolean;
  }
</script>

<script lang="ts">
  interface Props {
    slots: QueueSlot[];
    /** Names the list — "Slide queue". */
    label: string;
    /** The mono note on the right of the header — "4 of 8 filled". */
    summary?: string;
    openLabel: string;
    clearLabel: string;
    emptyLabel: string;
    onopen?: (slot: QueueSlot) => void;
    onclear?: (slot: QueueSlot) => void;
  }

  let {
    slots,
    label,
    summary = '',
    openLabel,
    clearLabel,
    emptyLabel,
    onopen,
    onclear,
  }: Props = $props();
</script>

<section class="queue" aria-label={label}>
  <header>
    <span>{label}</span>
    {#if summary}<em>{summary}</em>{/if}
  </header>
  <ul class="items">
    {#each slots as slot (slot.index)}
      <li class:filled={Boolean(slot.title)} class:loaded={slot.loaded}>
        <button
          class="open"
          type="button"
          disabled={!slot.title}
          aria-label={slot.title
            ? `${openLabel} ${slot.index}: ${slot.title}`
            : `${emptyLabel} ${slot.index}`}
          onclick={() => onopen?.(slot)}
        >
          <code>{slot.index}</code>
          <span>{slot.title ?? emptyLabel}</span>
        </button>
        {#if slot.title}
          <button
            class="clear"
            type="button"
            aria-label={`${clearLabel} ${slot.index}`}
            onclick={() => onclear?.(slot)}
          >
            ×
          </button>
        {/if}
      </li>
    {/each}
  </ul>
</section>

<style>
  .queue {
    color: var(--text-primary);
  }

  header {
    display: flex;
    justify-content: space-between;
    padding-bottom: var(--c-queue-header-padding);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  header span,
  header em,
  code {
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-queue-label-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
    font-style: normal;
  }

  .items {
    display: grid;
    gap: var(--c-queue-gap);
    padding: var(--c-queue-padding-top) 0 0;
    margin: 0;
    list-style: none;
  }

  li {
    display: grid;
    grid-template-columns: 1fr auto;
    border: var(--ui-border-hairline) dashed var(--border-control);
    color: var(--text-muted);
  }

  .filled {
    border-style: solid;
    color: var(--status-ok);
  }

  .loaded {
    background: var(--status-ok);
    color: var(--text-inverse);
  }

  button {
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  .open {
    min-width: 0;
    min-height: var(--ui-target-min);
    display: flex;
    align-items: center;
    gap: var(--c-queue-item-gap);
    padding: var(--c-queue-item-padding-block) var(--ui-gutter-tight);
    text-align: left;
  }

  .open:disabled {
    cursor: default;
  }

  .open span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .clear {
    min-width: var(--ui-target-min);
    border-left: var(--ui-border-hairline) solid currentColor;
    font-size: var(--c-queue-clear-size);
  }
</style>
