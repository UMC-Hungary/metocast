<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open?: boolean;
    title: string;
    eyebrow?: string;
    /**
     * A destructive confirmation must be dismissed deliberately, so the scrim and Escape stop
     * closing it.
     */
    modalOnly?: boolean;
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  }

  let {
    open = $bindable(false),
    title,
    eyebrow = '',
    modalOnly = false,
    onclose,
    children,
    footer,
  }: Props = $props();

  const id = `sanctum-dialog-${crypto.randomUUID()}`;

  function close() {
    if (!open) return;
    open = false;
    onclose?.();
  }
</script>

{#if open}
  <dialog
    class="shade"
    aria-labelledby={`${id}-title`}
    {@attach (el) => el.showModal()}
    onclose={close}
    oncancel={(event) => {
      event.preventDefault();
      if (!modalOnly) close();
    }}
    onclick={(event) => {
      if (!modalOnly && event.target === event.currentTarget) close();
    }}
  >
    <div class="panel">
      <header>
        {#if eyebrow}<small>{eyebrow}</small>{/if}
        <h2 id={`${id}-title`}>{title}</h2>
      </header>
      <div class="body">{@render children()}</div>
      {#if footer}<footer>{@render footer()}</footer>{/if}
    </div>
  </dialog>
{/if}

<style>
  .shade {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    max-width: none;
    max-height: none;
    margin: 0;
    border: 0;
    color: inherit;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--ui-gutter);
    background: var(--surface-scrim);
    overscroll-behavior: contain;
    animation: sanctum-fade var(--motion-slide) var(--motion-ease-default);
  }

  .shade::backdrop {
    background: transparent;
  }

  .panel {
    width: 100%;
    max-width: var(--c-dialog-max-width);
    max-height: 100%;
    overflow: auto;
    background: var(--surface-base);
    border: var(--ui-border-hairline) solid var(--border-strong);
    animation: sanctum-sheet var(--motion-enter) var(--motion-ease-standard);
  }

  header {
    padding: var(--c-sheet-header-padding-top) var(--ui-gutter) var(--c-sheet-header-padding-bottom);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  small {
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--type-label-track);
    text-transform: var(--type-label-transform);
    color: var(--text-muted);
  }

  h2 {
    margin: var(--c-sheet-title-gap) 0 0;
    font-family: var(--type-display-family);
    font-size: var(--c-sheet-title-size);
    font-weight: var(--type-display-weight);
    line-height: var(--c-sheet-title-leading);
    color: var(--text-primary);
  }

  .body {
    padding: var(--c-dialog-body-padding-block) var(--ui-gutter);
  }

  footer {
    display: flex;
    gap: var(--c-dialog-footer-gap);
    padding: 0 var(--ui-gutter) var(--ui-gutter);
  }

  @keyframes sanctum-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes sanctum-sheet {
    from {
      transform: translateY(28px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>
