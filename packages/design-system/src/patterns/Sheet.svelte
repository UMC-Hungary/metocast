<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open?: boolean;
    title: string;
    /** The mono micro-label above the title. */
    eyebrow?: string;
    /** Names the dialog when the title is not enough on its own. */
    ariaLabel?: string;
    /** Shows the drag handle. Its accessible name is what a keyboard user hears. */
    grabber?: boolean;
    grabberLabel?: string;
    /** Square the top corners for a sheet that reaches the top of the window. */
    rounded?: boolean;
    maxHeight?: string;
    onclose?: () => void;
    leading?: Snippet;
    action?: Snippet;
    children: Snippet;
  }

  let {
    open = $bindable(false),
    title,
    eyebrow = '',
    ariaLabel,
    grabber = true,
    grabberLabel = 'Drag down to close',
    rounded = true,
    maxHeight = '92%',
    onclose,
    leading,
    action,
    children,
  }: Props = $props();

  let sheet = $state<HTMLElement>();
  let dragging = $state(false);
  let dragY = $state(0);
  let startY = 0;
  let pointer: number | null = null;

  function close() {
    if (!open) return;
    dragging = false;
    pointer = null;
    dragY = 0;
    open = false;
    onclose?.();
  }

  function startDrag(event: PointerEvent) {
    if (event.button) return;
    dragging = true;
    pointer = event.pointerId;
    startY = event.clientY;
    dragY = 0;
    (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
    event.preventDefault();
  }

  function moveDrag(event: PointerEvent) {
    if (!dragging || event.pointerId !== pointer) return;
    dragY = Math.max(0, event.clientY - startY);
  }

  function endDrag(event: PointerEvent) {
    if (!dragging || event.pointerId !== pointer) return;
    const height = sheet?.getBoundingClientRect().height ?? 0;
    const threshold = Math.min(140, Math.max(88, height * 0.22));
    (event.currentTarget as HTMLElement).releasePointerCapture?.(event.pointerId);
    if (dragY > threshold) close();
    else {
      dragging = false;
      pointer = null;
      dragY = 0;
    }
  }
</script>

{#if open}
  <dialog
    class="shade"
    aria-label={ariaLabel ?? title}
    {@attach (el) => el.showModal()}
    onclose={close}
    oncancel={(event) => {
      event.preventDefault();
      close();
    }}
    onclick={(event) => {
      if (event.target === event.currentTarget) close();
    }}
  >
    <div
      class="panel"
      bind:this={sheet}
      class:dragging
      class:rounded
      style:max-height={maxHeight}
      style:--sanctum-sheet-drag={`${dragY}px`}
    >
      {#if grabber}
        <button
          class="grabber-hit"
          type="button"
          aria-label={grabberLabel}
          onpointerdown={startDrag}
          onpointermove={moveDrag}
          onpointerup={endDrag}
          onpointercancel={() => {
            dragging = false;
            dragY = 0;
          }}
          onclick={close}
        >
          <span class="grabber" aria-hidden="true"></span>
        </button>
      {/if}
      <header>
        {#if leading}{@render leading()}{/if}
        <div>
          {#if eyebrow}<small>{eyebrow}</small>{/if}
          <h2>{title}</h2>
        </div>
        {#if action}{@render action()}{/if}
      </header>
      {@render children()}
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
    padding: 0;
    border: 0;
    color: inherit;
    overflow: hidden;
    display: flex;
    align-items: flex-end;
    background: var(--surface-scrim);
    overscroll-behavior: contain;
    animation: sanctum-fade var(--motion-slide) var(--motion-ease-default);
  }

  .shade::backdrop {
    background: transparent;
  }

  .panel {
    width: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--surface-base);
    border-top: var(--ui-border-hairline) solid var(--border-strong);
    padding-bottom: var(--c-sheet-padding-bottom);
    overscroll-behavior: contain;
    transform: translateY(var(--sanctum-sheet-drag, 0));
    transition: transform var(--motion-base) var(--motion-ease-standard);
    animation: sanctum-sheet var(--motion-enter) var(--motion-ease-standard);
  }

  .dragging {
    transition: none;
  }

  .rounded {
    border-top-left-radius: var(--ui-radius-sheet);
    border-top-right-radius: var(--ui-radius-sheet);
  }

  .grabber-hit {
    width: 100%;
    height: var(--c-sheet-grabber-hit-height);
    padding: var(--c-sheet-grabber-padding-top) 0 var(--c-sheet-grabber-padding-bottom);
    border: 0;
    background: transparent;
    touch-action: none;
    cursor: grab;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .grabber-hit:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  .grabber {
    width: var(--c-sheet-grabber-width);
    height: var(--c-sheet-grabber-height);
    border-radius: var(--c-sheet-grabber-radius);
    background: var(--border-control);
    display: block;
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--ui-stack-loose);
    padding: var(--c-sheet-header-padding-top) var(--ui-gutter) var(--c-sheet-header-padding-bottom);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  header > div {
    flex: 1;
    min-width: 0;
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
