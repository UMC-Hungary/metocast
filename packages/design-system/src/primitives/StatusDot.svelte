<script lang="ts" module>
  export type Status = 'live' | 'ok' | 'warn' | 'error' | 'off';

  const colors: Record<Status, string> = {
    live: 'var(--status-live)',
    ok: 'var(--status-ok)',
    warn: 'var(--status-warn)',
    error: 'var(--status-error)',
    off: 'var(--status-off)',
  };
</script>

<script lang="ts">
  import Dot from './Dot.svelte';

  interface Props {
    status: Status;
    /**
     * The word beside the dot. Required, because the colour alone must never be what tells
     * a reader the state — the reference pairs every dot with one.
     */
    label: string;
    /** Hides the label visually while leaving it to assistive technology. */
    labelHidden?: boolean;
    size?: number;
  }

  let { status, label, labelHidden = false, size = 6 }: Props = $props();
</script>

<span class="status">
  <Dot color={colors[status]} {size} pulse={status === 'live'} />
  <span class:visually-hidden={labelHidden}>{label}</span>
</span>

<style>
  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--c-status-dot-gap);
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--type-label-sm-track);
    font-weight: var(--type-label-sm-weight);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }
</style>
