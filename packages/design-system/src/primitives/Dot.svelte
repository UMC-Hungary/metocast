<script lang="ts">
  interface Props {
    /** A semantic colour token reference, never a literal. */
    color?: string;
    size?: number;
    /** The slow ring the reference uses for a live signal. Stilled under reduced motion. */
    pulse?: boolean;
  }

  let { color = 'var(--text-primary)', size = 7, pulse = false }: Props = $props();
</script>

<span class="dot" style:--sanctum-dot-color={color} style:--sanctum-dot-size={`${size}px`}>
  {#if pulse}<span class="pulse"></span>{/if}
</span>

<style>
  .dot {
    position: relative;
    display: inline-block;
    width: var(--sanctum-dot-size);
    height: var(--sanctum-dot-size);
    border-radius: var(--ui-radius-circle);
    background: var(--sanctum-dot-color);
    flex: 0 0 auto;
  }

  .pulse {
    position: absolute;
    inset: var(--c-dot-pulse-inset);
    border-radius: var(--ui-radius-circle);
    border: var(--c-dot-pulse-border) solid var(--sanctum-dot-color);
    animation: sanctum-pulse var(--motion-pulse) var(--motion-ease-out) infinite;
  }

  @keyframes sanctum-pulse {
    0% {
      transform: scale(1);
      opacity: 0.55;
    }
    70%,
    100% {
      transform: scale(2.4);
      opacity: 0;
    }
  }
</style>
