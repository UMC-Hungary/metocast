<script lang="ts">
  import type { Snippet } from 'svelte';
  import Dot from './Dot.svelte';
  import type { Status } from './StatusDot.svelte';

  interface Props {
    /** Tints the outline and the leading dot. `neutral` carries no dot. */
    tone?: Status | 'neutral';
    /** Shows the leading dot, as the reference's state chip does. */
    dot?: boolean;
    children: Snippet;
  }

  let { tone = 'neutral', dot = false, children }: Props = $props();

  const colors: Record<string, string> = {
    live: 'var(--status-live)',
    ok: 'var(--status-ok)',
    warn: 'var(--status-warn)',
    error: 'var(--status-error)',
    off: 'var(--status-off)',
    neutral: 'var(--text-muted)',
  };
</script>

<span class="badge {tone}">
  {#if dot && tone !== 'neutral'}<Dot color={colors[tone]} size={4} />{/if}
  {@render children()}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--c-badge-gap);
    padding: var(--c-badge-padding-block) var(--c-badge-padding-inline);
    border: var(--ui-border-hairline) solid currentColor;
    font-family: var(--type-label-xs-family);
    font-size: var(--type-label-xs-size);
    letter-spacing: var(--c-badge-track);
    text-transform: var(--type-label-xs-transform);
    white-space: nowrap;
  }

  .neutral {
    color: var(--text-muted);
    border-color: var(--border-control);
  }

  .live {
    color: var(--status-live);
  }

  .ok {
    color: var(--status-ok);
  }

  .warn {
    color: var(--status-warn);
  }

  .error {
    color: var(--status-error);
  }

  .off {
    color: var(--status-off);
  }
</style>
