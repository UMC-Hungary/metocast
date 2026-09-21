<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** The text shown on hover and focus. Never the only place the information lives. */
    text: string;
    placement?: 'top' | 'bottom';
    children: Snippet<[{ describedby: string }]>;
  }

  let { text, placement = 'top', children }: Props = $props();

  const id = `sanctum-tooltip-${crypto.randomUUID()}`;
  let open = $state(false);

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && open) {
      event.stopPropagation();
      open = false;
    }
  }
</script>

<!--
  Hover and focus both show it, Escape dismisses it, and it stays open while the pointer is over
  it — the three things WCAG 1.4.13 asks of content that appears on hover.
-->
<span
  class="wrap"
  role="presentation"
  onmouseenter={() => (open = true)}
  onmouseleave={() => (open = false)}
  onfocusin={() => (open = true)}
  onfocusout={() => (open = false)}
  {onkeydown}
>
  {@render children({ describedby: id })}
  <span {id} role="tooltip" class="bubble {placement}" class:open>{text}</span>
</span>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
  }

  .bubble {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    z-index: var(--z-tooltip);
    display: none;
    max-width: var(--c-tooltip-max-width);
    padding: var(--c-tooltip-padding-block) var(--c-tooltip-padding-inline);
    background: var(--surface-inverse);
    color: var(--text-inverse);
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--type-label-sm-track);
    line-height: var(--type-caption-leading);
    white-space: normal;
    pointer-events: auto;
  }

  .open {
    display: block;
  }

  .top {
    bottom: 100%;
    margin-bottom: var(--c-tooltip-offset);
  }

  .bottom {
    top: 100%;
    margin-top: var(--c-tooltip-offset);
  }
</style>
