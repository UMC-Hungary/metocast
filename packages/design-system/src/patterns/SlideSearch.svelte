<script lang="ts" module>
  export interface SlideResult {
    id: string;
    /** The deck or folder the slide belongs to. */
    group: string;
    title: string;
    /** A short position marker — "12 / 40". */
    position?: string;
  }
</script>

<script lang="ts">
  import Icon from '../primitives/Icon.svelte';

  interface Props {
    results: SlideResult[];
    /** Names the search region. */
    label: string;
    /** Mono eyebrow inside the trigger — "SEARCH FILES". */
    searchLabel: string;
    /** Placeholder shown in the trigger when filter is empty. */
    placeholder: string;
    /** Current filter value — bindable so parent can run search. */
    filter?: string;
    emptyMessage: string;
    openLabel: string;
    queueLabel: string;
    queueDisabled?: boolean;
    /** Always render the digit numpad (for mobile). */
    numpad?: boolean;
    ontrigger?: () => void;
    onopen?: (result: SlideResult) => void;
    onqueue?: (result: SlideResult) => void;
  }

  let {
    results,
    label,
    searchLabel,
    placeholder,
    filter = $bindable(''),
    emptyMessage,
    openLabel,
    queueLabel,
    queueDisabled = false,
    numpad = false,
    ontrigger,
    onopen,
    onqueue,
  }: Props = $props();

  const keys = ['1', '2', '3', '⌫', '4', '5', '6', 'CLR', '7', '8', '9', '0'];

  function press(ch: string) {
    if (ch === 'CLR') filter = '';
    else if (ch === '⌫') filter = filter.slice(0, -1);
    else filter = (filter + ch).slice(0, 6);
  }
</script>

<section class="search" aria-label={label}>
  <button class="trigger" type="button" onclick={ontrigger}>
    <Icon name="search" size={18} stroke={1.6} />
    <span>
      <em>{searchLabel}</em>
      <strong>{filter || placeholder}</strong>
    </span>
    <em class="right">{openLabel}</em>
  </button>

  {#if !numpad}
    <input class="text" type="search" bind:value={filter} {placeholder} aria-label={searchLabel} />
  {/if}

  {#if results.length === 0 && filter.length > 0}
    <p class="empty">{emptyMessage}</p>
  {:else if results.length > 0}
    <ul class="results">
      {#each results as result (result.id)}
        <li>
          <button
            class="open"
            type="button"
            aria-label={`${openLabel}: ${result.title}`}
            onclick={() => onopen?.(result)}
          >
            <code>{result.position ?? ''}</code>
            <span>{result.title}</span>
            <em>{result.group}</em>
          </button>
          <button
            class="queue"
            type="button"
            disabled={queueDisabled}
            aria-label={`${queueLabel}: ${result.title}`}
            onclick={() => onqueue?.(result)}
          >
            +
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if numpad}
    <div class="digits">
      {#each keys as ch (ch)}
        <button
          type="button"
          class:danger={ch === '⌫'}
          class:warn={ch === 'CLR'}
          onclick={() => press(ch)}>{ch}</button
        >
      {/each}
    </div>
  {/if}
</section>

<style>
  .trigger {
    width: 100%;
    min-height: var(--ui-target-min);
    display: flex;
    align-items: center;
    gap: var(--ui-gutter-tight);
    padding: var(--ui-gutter-inset);
    background: var(--surface-raised);
    border: var(--ui-border-hairline) solid var(--border-control);
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
  }

  .trigger span {
    flex: 1;
    min-width: 0;
  }

  em,
  code {
    display: block;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-search-label-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
    font-style: normal;
  }

  .trigger .right {
    flex-shrink: 0;
  }

  strong {
    display: block;
    margin-top: var(--c-search-title-gap);
    font-family: var(--type-body-family);
    font-size: var(--type-body-size);
    font-weight: var(--type-body-strong-weight);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .results {
    margin: 0;
    padding: 0;
    list-style: none;
    border-left: var(--ui-border-hairline) solid var(--border-control);
    border-right: var(--ui-border-hairline) solid var(--border-control);
    background: var(--surface-raised);
  }

  li {
    display: grid;
    grid-template-columns: 1fr auto;
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  .open {
    min-width: 0;
    min-height: var(--ui-target-min);
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--c-search-item-row-gap) var(--c-search-item-column-gap);
    padding: var(--c-search-item-padding-block) var(--ui-gutter-inset);
    border: 0;
    background: transparent;
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
  }

  .open span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .open em {
    grid-column: 2;
  }

  .queue {
    min-width: var(--ui-target-min);
    padding: 0 var(--ui-gutter-tight);
    border: 0;
    border-left: var(--ui-border-hairline) solid var(--border-hairline);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
  }

  .queue:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  .text {
    display: block;
    width: 100%;
    box-sizing: border-box;
    padding: var(--c-search-item-padding-block) var(--ui-gutter-inset);
    background: var(--surface-raised);
    border: var(--ui-border-hairline) solid var(--border-control);
    border-top: 0;
    color: var(--text-primary);
    font-family: var(--type-body-family);
    font-size: var(--type-body-size);
    outline: none;
  }

  .empty {
    margin: 0;
    padding: var(--c-search-empty-padding-block) var(--ui-gutter-inset);
    color: var(--text-muted);
    font-family: var(--type-quote-family);
    font-style: italic;
  }

  .digits {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--space-8);
    margin-top: var(--space-12);
    margin-bottom: var(--ui-gutter-inset);
    padding: 0 var(--ui-gutter-inset);
  }

  .digits button {
    min-height: var(--size-48);
    border: var(--ui-border-hairline) solid var(--border-control);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    font-family: var(--type-title-family);
    font-size: var(--type-title-size);
    font-weight: var(--type-title-weight);
  }

  .digits .danger,
  .digits .warn {
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--c-dock-button-label-track);
    text-transform: var(--type-label-transform);
  }

  .digits .danger {
    color: var(--status-error);
  }

  .digits .warn {
    color: var(--status-warn);
  }
</style>
