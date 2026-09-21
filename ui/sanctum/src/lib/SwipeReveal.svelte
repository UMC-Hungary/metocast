<script module lang="ts">
  // Only one row's reveal panel stays open at a time; opening another closes it.
  let openClose: (() => void) | null = null;
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Icon } from '@metocast/design-system';

  interface Props {
    /** Runs on commit, whether triggered by the revealed button or by swiping past the auto-commit distance. */
    onCommit: () => void;
    /** Names the revealed button for screen readers; the row's own delete control stays the accessible path. */
    commitLabel: string;
    children: Snippet;
  }

  let { onCommit, commitLabel, children }: Props = $props();

  const REVEAL = 88;
  const AUTO_COMMIT = 230;

  let fg = $state<HTMLElement>();
  let offset = $state(0);
  let dragging = $state(false);
  let pointerId: number | null = null;
  let startX = 0;
  let startY = 0;
  let baseOffset = 0;
  let axis: 'x' | 'y' | null = null;

  function close() {
    offset = 0;
    if (openClose === close) openClose = null;
  }

  function clamp(value: number) {
    if (value <= REVEAL) return Math.max(0, value);
    return REVEAL + (value - REVEAL) * 0.55;
  }

  function startDrag(event: PointerEvent) {
    if (event.button) return;
    pointerId = event.pointerId;
    startX = event.clientX;
    startY = event.clientY;
    baseOffset = offset;
    axis = null;
    if (openClose && openClose !== close) openClose();
  }

  function moveDrag(event: PointerEvent) {
    if (pointerId !== event.pointerId) return;
    const rawX = event.clientX - startX;
    const rawY = event.clientY - startY;
    if (!axis) {
      if (Math.abs(rawX) < 8 && Math.abs(rawY) < 8) return;
      axis = Math.abs(rawX) > Math.abs(rawY) ? 'x' : 'y';
      if (axis === 'x') {
        fg?.setPointerCapture(pointerId);
        dragging = true;
      }
    }
    if (axis !== 'x') return;
    event.preventDefault();
    // Dragging right subtracts from the offset the drag started at, so an already-open
    // row follows the finger back closed instead of only ever opening further.
    offset = clamp(Math.max(0, baseOffset - rawX));
  }

  function endDrag(event: PointerEvent) {
    if (pointerId !== event.pointerId) return;
    pointerId = null;
    dragging = false;
    const wasDragging = axis === 'x';
    axis = null;
    if (!wasDragging) return;
    if (offset > AUTO_COMMIT) {
      onCommit();
      offset = 0;
    } else if (offset > REVEAL / 2) {
      offset = REVEAL;
      openClose = close;
    } else {
      close();
    }
  }
</script>

<div class="swipe-item">
  <div class="swipe-bg">
    <button class="swipe-commit" type="button" aria-label={commitLabel} onclick={onCommit}>
      <Icon name="close" size={20} stroke={1.8} />
    </button>
  </div>
  <div
    class="swipe-fg"
    class:dragging
    role="group"
    bind:this={fg}
    style:transform={`translateX(${-offset}px)`}
    onpointerdown={startDrag}
    onpointermove={moveDrag}
    onpointerup={endDrag}
    onpointercancel={endDrag}
  >
    {@render children()}
  </div>
</div>

<style>
  .swipe-item {
    position: relative;
    overflow: hidden;
  }

  .swipe-bg {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: stretch;
    justify-content: flex-end;
    background: var(--status-error);
  }

  .swipe-commit {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 88px;
    flex: 0 0 auto;
    border: 0;
    background: transparent;
    color: var(--text-inverse);
    cursor: pointer;
  }

  .swipe-fg {
    position: relative;
    background: var(--surface-raised);
    touch-action: pan-y;
    transition: transform var(--motion-base) var(--motion-ease-standard);
  }

  .swipe-fg.dragging {
    transition: none;
  }
</style>
