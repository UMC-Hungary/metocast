<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  interface Props {
    title: string;
    /** The mono micro-label above the title. Ignored when `back` is given, as in the reference. */
    eyebrow?: string;
    /** Replaces the eyebrow with a back control. */
    back?: { label: string; onclick: () => void } | { label: string; href: string };
    /** Heading level. A screen has one h1; a header inside a region takes h2. */
    level?: 1 | 2;
    id?: string;
    eyebrowContent?: Snippet;
    trailing?: Snippet;
    titleTrailing?: Snippet;
  }

  let {
    title,
    eyebrow = '',
    back,
    level = 1,
    id,
    eyebrowContent,
    trailing,
    titleTrailing,
  }: Props = $props();
</script>

<header>
  <div class="top">
    {#if back}
      {#if 'href' in back}
        <a class="back" href={back.href}>
          <Icon name="back" size={18} stroke={1.6} />{back.label}
        </a>
      {:else}
        <button class="back" type="button" onclick={back.onclick}>
          <Icon name="back" size={18} stroke={1.6} />{back.label}
        </button>
      {/if}
    {:else if eyebrowContent}
      <div>{@render eyebrowContent()}</div>
    {:else}
      <div class="eyebrow">{eyebrow}</div>
    {/if}
    <div class="actions">
      {#if trailing}{@render trailing()}{/if}
    </div>
  </div>
  <div class="title-row">
    {#if level === 1}
      <h1 {id}>{title}</h1>
    {:else}
      <h2 {id}>{title}</h2>
    {/if}
    {#if titleTrailing}{@render titleTrailing()}{/if}
  </div>
</header>

<style>
  header {
    padding: var(--ui-gutter) var(--ui-gutter) var(--c-page-header-padding-bottom);
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: var(--c-page-header-top-min-height);
    margin-bottom: var(--c-page-header-top-gap);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--ui-stack);
    min-width: 0;
  }

  .eyebrow,
  .back {
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--type-label-track);
    text-transform: var(--type-label-transform);
    color: var(--text-muted);
  }

  .back {
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: var(--c-page-header-back-gap);
    color: var(--text-primary);
    text-decoration: none;
    font-family: var(--type-body-sm-family);
    font-size: var(--type-body-sm-size);
    letter-spacing: var(--ui-track-none);
    text-transform: none;
  }

  .back:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  h1,
  h2 {
    margin: 0;
    font-family: var(--type-display-family);
    font-size: var(--type-display-size);
    line-height: var(--type-display-leading);
    font-weight: var(--type-display-weight);
    color: var(--text-primary);
    letter-spacing: var(--type-display-track);
  }

  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--c-page-header-title-gap);
  }
</style>
