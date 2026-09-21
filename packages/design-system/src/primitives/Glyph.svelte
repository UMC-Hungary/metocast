<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /**
     * The mark inside the tile. A brand logo is supplied by the caller as an inline SVG snippet —
     * the reference masks one in from a CDN, which an offline application cannot do.
     */
    mark?: Snippet;
    /** The typographic fallback the reference uses when it has no logo for a kind. */
    char?: string;
    size?: number;
    /** Names the tile when it is the only thing identifying what it stands for. */
    label?: string;
  }

  let { mark, char = '·', size = 34, label }: Props = $props();
</script>

<div
  class="glyph"
  style:--sanctum-glyph-size={`${size}px`}
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  {#if mark}{@render mark()}{:else}<span class="char">{char}</span>{/if}
</div>

<style>
  .glyph {
    width: var(--sanctum-glyph-size);
    height: var(--sanctum-glyph-size);
    border: var(--ui-border-hairline) solid var(--border-strong);
    border-radius: var(--ui-radius-mark);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--text-primary);
  }

  .char {
    font-family: var(--type-heading-family);
    font-size: calc(var(--sanctum-glyph-size) * 0.5);
    font-weight: var(--type-heading-weight);
    line-height: var(--type-numeral-leading);
  }
</style>
