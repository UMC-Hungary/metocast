<script lang="ts">
  interface Props {
    primary: string;
    secondary?: string;
    onprimary?: () => void;
    onsecondary?: () => void;
    primaryDisabled?: boolean;
    /** How far above the bottom edge the bar settles. */
    bottom?: string;
  }

  let {
    primary,
    secondary = '',
    onprimary,
    onsecondary,
    primaryDisabled = false,
    bottom = '24px',
  }: Props = $props();
</script>

<div class="bar" style:--sanctum-actionbar-bottom={bottom}>
  {#if secondary}
    <button class="secondary" type="button" onclick={onsecondary}>{secondary}</button>
  {/if}
  <button class="primary" type="button" disabled={primaryDisabled} onclick={onprimary}>
    {primary}
  </button>
</div>

<style>
  .bar {
    position: sticky;
    left: 0;
    right: 0;
    bottom: var(--sanctum-actionbar-bottom);
    z-index: var(--z-actionbar);
    display: flex;
    gap: var(--ui-stack);
    padding: var(--c-action-bar-padding-block) var(--c-action-bar-padding-inline);
    padding-bottom: max(var(--c-action-bar-padding-block), env(safe-area-inset-bottom));
    background: var(--surface-base);
    border-top: var(--ui-border-hairline) solid var(--border-strong);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  button {
    min-height: var(--c-action-bar-button-height);
    padding: 0 var(--c-action-bar-button-padding);
    cursor: pointer;
    font-family: var(--type-body-sm-family);
    font-weight: var(--type-body-sm-weight);
    letter-spacing: var(--c-action-bar-track);
    white-space: nowrap;
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  .secondary {
    background: transparent;
    color: var(--text-primary);
    border: var(--ui-border-hairline) solid var(--border-control);
  }

  .primary {
    flex: 1;
    min-width: 0;
    background: var(--surface-inverse);
    color: var(--text-inverse);
    border: 0;
  }

  .primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
