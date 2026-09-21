<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    /** Required unless the toggle is labelled by another element through `labelledby`. */
    label?: string;
    labelledby?: string;
    describedby?: string;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    disabled = false,
    label,
    labelledby,
    describedby,
    onchange,
  }: Props = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  aria-labelledby={labelledby}
  aria-describedby={describedby}
  class:checked
  {disabled}
  onclick={toggle}
>
  <span class="thumb"></span>
</button>

<style>
  button {
    width: var(--c-toggle-width);
    height: var(--c-toggle-height);
    /* No border — the reference toggle is borderless; a border would also eat the
       inner height under border-box and push the thumb off-centre. */
    border: 0;
    border-radius: var(--ui-radius-pill);
    padding: 0;
    background: var(--border-strong);
    cursor: pointer;
    position: relative;
    flex-shrink: 0;
    transition:
      background var(--motion-base) var(--motion-ease-default),
      opacity var(--motion-base) var(--motion-ease-default);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  button.checked {
    background: var(--surface-inverse);
    border-color: var(--surface-inverse);
  }

  .thumb {
    position: absolute;
    top: var(--c-toggle-thumb-inset);
    left: var(--c-toggle-thumb-inset);
    width: var(--c-toggle-thumb-size);
    height: var(--c-toggle-thumb-size);
    border-radius: var(--ui-radius-circle);
    background: var(--surface-raised);
    transition: left var(--motion-slide) var(--motion-ease-standard);
  }

  .checked .thumb {
    left: var(--c-toggle-thumb-travel);
  }
</style>
