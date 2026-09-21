<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Names the region so it can be reached from the landmark list. */
    label: string;
    /**
     * `polite` for the ordinary case; a toast that reports a failure the user must act on
     * belongs in an `assertive` region, or better, in an `ErrorState` on the surface itself.
     */
    priority?: 'polite' | 'assertive';
    children: Snippet;
  }

  let { label, priority = 'polite', children }: Props = $props();
</script>

<div
  class="overlay"
  role="region"
  aria-label={label}
  aria-live={priority}
  aria-relevant="additions"
>
  {@render children()}
</div>

<style>
  .overlay {
    position: fixed;
    top: var(--c-toast-overlay-top);
    left: var(--c-toast-overlay-inset);
    right: var(--c-toast-overlay-inset);
    z-index: var(--z-toast);
    display: flex;
    flex-direction: column;
    gap: var(--ui-stack);
    pointer-events: none;
  }

  .overlay > :global(*) {
    pointer-events: auto;
    animation: sanctum-toast-in var(--motion-enter) var(--motion-ease-standard);
    box-shadow: 0 var(--ui-stack) var(--c-toast-overlay-shadow-blur) var(--shadow-overlay);
  }

  @keyframes sanctum-toast-in {
    from {
      transform: translateY(-12px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>
