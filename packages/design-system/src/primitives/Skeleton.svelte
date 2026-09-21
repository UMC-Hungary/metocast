<script lang="ts">
  interface Props {
    /** `text` matches a body line, `label` a mono micro-label, `block` an arbitrary box. */
    shape?: 'text' | 'label' | 'block';
    /** CSS width. A skeleton stands in for content whose width is not known. */
    width?: string;
    height?: string;
    lines?: number;
  }

  let { shape = 'text', width = '100%', height, lines = 1 }: Props = $props();
</script>

<!--
  One `aria-hidden` placeholder set. The surface that is loading announces the wait through its
  own live region; a screen reader has nothing to gain from hearing the shape of the wait.
-->
<div class="skeleton" aria-hidden="true" style:--sanctum-skeleton-width={width}>
  {#each { length: lines } as _, index (index)}
    <span
      class={shape}
      style:height
      style:width={index === lines - 1 && lines > 1 ? '62%' : undefined}
    ></span>
  {/each}
</div>

<style>
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--c-skeleton-gap);
    width: var(--sanctum-skeleton-width);
  }

  span {
    display: block;
    width: 100%;
    background: var(--surface-sunken);
    animation: sanctum-skeleton var(--motion-pulse) var(--motion-ease-default) infinite;
  }

  .text {
    height: var(--c-skeleton-text-height);
  }

  .label {
    height: var(--c-skeleton-label-height);
  }

  .block {
    height: var(--c-skeleton-block-height);
  }

  @keyframes sanctum-skeleton {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.55;
    }
  }
</style>
