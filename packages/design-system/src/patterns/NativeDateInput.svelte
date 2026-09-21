<script lang="ts">
  import Icon, { type IconName } from '../primitives/Icon.svelte';

  interface Props {
    /** The visible label — the reference shows the formatted value here. */
    label: string;
    /** The icon affordance on the right, hinting the field opens a picker. */
    icon: IconName;
    type?: 'date' | 'time' | 'datetime-local';
    value?: string;
    disabled?: boolean;
    id?: string;
    /** Names the control for assistive technology, which the visible value alone does not. */
    accessibleName: string;
  }

  let {
    label,
    icon,
    type = 'date',
    value = $bindable(''),
    disabled = false,
    id = `sanctum-native-date-${crypto.randomUUID()}`,
    accessibleName,
  }: Props = $props();
</script>

<div class="native">
  <label for={id}>
    <span aria-hidden="true">{label}</span>
    <Icon name={icon} size={22} />
    <span class="visually-hidden">{accessibleName}</span>
  </label>
  <input {id} {type} {disabled} bind:value />
</div>

<style>
  .native {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ui-stack);
    min-height: var(--ui-target-min);
    padding: var(--c-text-field-padding-block) var(--ui-gutter-inset);
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    border-radius: var(--ui-radius-square);
  }

  label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ui-stack);
    width: 100%;
    cursor: pointer;
    font-family: var(--type-body-strong-family);
    font-size: var(--type-body-strong-size);
    color: var(--text-primary);
    font-weight: var(--type-body-strong-weight);
    letter-spacing: var(--type-body-strong-track);
  }

  label :global(svg) {
    color: var(--text-muted);
  }

  /*
   * The reference hides the input entirely, which leaves the control unreachable by keyboard.
   * It stays transparent and full-bleed so the whole row is the target, but keeps its box so
   * the focus indicator has somewhere to land.
   */
  input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
    width: 100%;
    border: 0;
    padding: 0;
    margin: 0;
  }

  /*
   * Revealed on any focus, not just :focus-visible — macOS Safari doesn't grant
   * :focus-visible to a date/time widget on a mouse click, only on keyboard focus,
   * which left the control focused but invisible and unusable with a pointer.
   */
  input:focus {
    opacity: 1;
  }

  input:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
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
