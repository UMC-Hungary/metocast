<script lang="ts">
  import FlameMark from './FlameMark.svelte';

  interface Props {
    /** The wordmark. A product name is not a translatable string, so it is passed in, not built in. */
    name: string;
    markSize?: number;
    fontSize?: number;
    /** Tracking in px. The reference varies it with the wordmark's size. */
    tracking?: number;
  }

  let { name, markSize = 18, fontSize = 17, tracking = 1 }: Props = $props();
</script>

<span
  class="lockup"
  style:--sanctum-lockup-size={`${fontSize}px`}
  style:--sanctum-lockup-track={`${tracking}px`}
>
  <FlameMark size={markSize} />
  <span class="word">{name}</span>
</span>

<style>
  .lockup {
    display: inline-flex;
    align-items: center;
    gap: var(--ui-stack);
  }

  .word {
    font-family: var(--type-heading-family);
    font-size: var(--sanctum-lockup-size);
    font-weight: var(--type-heading-weight);
    font-style: italic;
    color: var(--text-primary);
    letter-spacing: var(--sanctum-lockup-track);
    line-height: var(--type-numeral-leading);
  }
</style>
