<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  interface Props {
    title?: string;
    /** The second line, one tone lighter. Truncates rather than wrapping. */
    meta?: string;
    /** A trailing value, aligned to the row's right edge alongside the chevron. */
    detail?: string;
    chevron?: boolean;
    /** Renders the title in the danger colour; pair it with wording that says so too. */
    danger?: boolean;
    /** Drops the bottom rule, for the last row in a list. */
    last?: boolean;
    /**
     * Makes the row a button. A row that navigates should instead be given `href`, so it lands
     * in the keyboard order as the link it is.
     */
    onclick?: (event: MouseEvent) => void;
    href?: string;
    /** Marks the row as the current page or step; the reference has no such indication. */
    current?: 'page' | 'step' | 'true' | false;
    disabled?: boolean;
    icon?: Snippet;
    control?: Snippet;
    children?: Snippet;
  }

  let {
    title = '',
    meta = '',
    detail = '',
    chevron = true,
    danger = false,
    last = false,
    onclick,
    href,
    current = false,
    disabled = false,
    icon,
    control,
    children,
  }: Props = $props();

  const interactive = $derived(Boolean(onclick || href));
</script>

{#snippet body()}
  {#if icon}{@render icon()}{/if}
  <span class="body">
    <span class="title"
      >{#if children}{@render children()}{:else}{title}{/if}</span
    >
    {#if meta}<span class="meta">{meta}</span>{/if}
  </span>
  {#if detail}<span class="detail">{detail}</span>{/if}
  {#if control}{@render control()}{/if}
  {#if interactive}
    <span class="chev"
      >{#if chevron}<Icon name="chev" size={14} stroke={1.6} />{/if}</span
    >
  {/if}
{/snippet}

{#if href}
  <a class="row" class:danger class:last {href} aria-current={current || undefined}>
    {@render body()}
  </a>
{:else if onclick}
  <button
    class="row"
    class:danger
    class:last
    type="button"
    {disabled}
    {onclick}
    aria-current={current || undefined}
  >
    {@render body()}
  </button>
{:else}
  <div class="row" class:danger class:last>{@render body()}</div>
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--ui-stack-loose);
    padding: var(--c-row-padding-block) var(--ui-gutter);
    min-height: var(--c-row-min-height);
    border: 0;
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
    transition: background var(--motion-fast);
    width: 100%;
    background: transparent;
    color: inherit;
    text-align: left;
    text-decoration: none;
    font-family: inherit;
  }

  .last {
    border-bottom: 0;
  }

  a.row,
  button.row {
    cursor: pointer;
  }

  a.row:hover,
  button.row:not(:disabled):hover {
    background: var(--surface-hover);
  }

  button.row:disabled {
    cursor: not-allowed;
    color: var(--text-muted);
  }

  /* The reference has no focus indicator at all; see the recorded deviation. */
  .row:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  .chev {
    color: var(--text-faint);
    display: flex;
    justify-content: center;
    flex: 0 0 var(--size-14);
  }

  .body {
    display: block;
    flex: 1;
    min-width: 0;
  }

  .title {
    display: block;
    font-family: var(--type-body-family);
    font-size: var(--type-body-size);
    color: var(--text-primary);
    letter-spacing: var(--type-body-track);
    font-weight: var(--type-body-weight);
    line-height: var(--type-body-leading);
    overflow-wrap: anywhere;
    word-break: normal;
  }

  .danger .title {
    color: var(--status-error);
  }

  .meta {
    display: block;
    font-size: var(--type-caption-size);
    color: var(--text-muted);
    margin-top: var(--c-row-meta-gap);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail {
    font-size: var(--type-body-sm-size);
    color: var(--text-muted);
    white-space: nowrap;
    margin-left: auto;
    text-align: right;
  }
</style>
