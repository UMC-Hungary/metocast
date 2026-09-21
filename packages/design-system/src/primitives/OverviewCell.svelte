<script lang="ts">
  import Dot from './Dot.svelte';

  interface Props {
    label: string;
    value: string | number;
    /** A semantic colour token reference for the leading dot. */
    color?: string;
    /** The word the dot's colour stands for, so the state is not carried by colour alone. */
    state?: string;
    /** Draws the rule that separates this cell from the one before it. */
    divider?: boolean;
    pulse?: boolean;
  }

  let {
    label,
    value,
    color = 'var(--text-primary)',
    state,
    divider = false,
    pulse = false,
  }: Props = $props();
</script>

<div class:divider>
  <p>
    <Dot {color} size={6} {pulse} />
    <span>{label}</span>
    {#if state}<span class="visually-hidden">{state}</span>{/if}
  </p>
  <strong>{value}</strong>
</div>

<style>
  div {
    padding: var(--c-overview-cell-padding-block) var(--ui-gutter-inset);
  }

  .divider {
    border-left: var(--ui-border-hairline) solid var(--border-hairline);
  }

  p {
    margin: 0 0 var(--c-overview-cell-gap);
    display: flex;
    align-items: center;
    gap: var(--c-status-dot-gap);
  }

  span {
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-overview-cell-label-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
  }

  strong {
    display: block;
    font-family: var(--type-title-family);
    font-size: var(--type-title-size);
    color: var(--text-primary);
    line-height: var(--type-title-leading);
    font-weight: var(--type-title-weight);
    letter-spacing: var(--type-title-track);
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
