<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    icon: IconName;
    /** Required: an icon-only control has no other name. */
    label: string;
    /** `circle` is the reference's outlined 38px settings button; `bare` is its header icon. */
    variant?: 'bare' | 'circle';
    size?: number;
    disabled?: boolean;
    onclick?: (event: MouseEvent) => void;
    href?: string;
  }

  let {
    icon,
    label,
    variant = 'bare',
    size = 20,
    disabled = false,
    onclick,
    href,
  }: Props = $props();
</script>

{#if href}
  <a class="btn {variant}" {href} aria-label={label}>
    <Icon name={icon} {size} />
  </a>
{:else}
  <button class="btn {variant}" type="button" aria-label={label} {disabled} {onclick}>
    <Icon name={icon} {size} />
  </button>
{/if}

<style>
  .btn {
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    /* The reference's 6px padding leaves a 32px target; the design's own minimum is 44. */
    min-width: var(--ui-target-min);
    min-height: var(--ui-target-min);
  }

  .bare {
    border: 0;
    padding: var(--c-icon-button-padding);
  }

  .circle {
    width: var(--c-icon-button-circle-size);
    height: var(--c-icon-button-circle-size);
    border-radius: var(--ui-radius-pill);
    border: var(--ui-border-hairline) solid var(--border-control);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }
</style>
