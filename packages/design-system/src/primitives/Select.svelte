<script lang="ts" module>
  export interface SelectOption<T extends string = string> {
    value: T;
    label: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string">
  import { dismissable } from '../a11y/dismissable.js';
  import Icon from './Icon.svelte';

  interface Props {
    options: SelectOption<T>[];
    value: T;
    /** Only when the select stands outside a `FormField`. */
    label?: string;
    id?: string;
    describedby?: string;
    disabled?: boolean;
    invalid?: boolean;
    placeholder?: string;
    onchange?: (value: T) => void;
  }

  let {
    options,
    value = $bindable(),
    label,
    id = `sanctum-select-${crypto.randomUUID()}`,
    describedby,
    disabled = false,
    invalid = false,
    placeholder = '',
    onchange,
  }: Props = $props();

  let open = $state(false);
  let active = $state(0);
  let trigger = $state<HTMLButtonElement>();
  let list = $state<HTMLElement>();

  const selectedIndex = $derived(
    Math.max(
      0,
      options.findIndex((o) => o.value === value),
    ),
  );
  const selected = $derived(options[selectedIndex]);

  function show() {
    if (disabled) return;
    active = selectedIndex;
    open = true;
  }

  function close(returnFocus = true) {
    open = false;
    if (returnFocus) trigger?.focus();
  }

  function commit(index: number) {
    const option = options[index];
    if (!option || option.disabled) return;
    value = option.value;
    onchange?.(option.value);
    close();
  }

  function step(delta: number) {
    let next = active;
    for (let i = 0; i < options.length; i += 1) {
      next = Math.min(options.length - 1, Math.max(0, next + delta));
      if (!options[next]?.disabled) break;
      if (next === 0 || next === options.length - 1) break;
    }
    active = next;
  }

  function onTriggerKeydown(event: KeyboardEvent) {
    if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) {
      event.preventDefault();
      show();
    }
  }

  function onListKeydown(event: KeyboardEvent) {
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        step(1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        step(-1);
        break;
      case 'Home':
        event.preventDefault();
        active = 0;
        break;
      case 'End':
        event.preventDefault();
        active = options.length - 1;
        break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        commit(active);
        break;
      case 'Tab':
        close(false);
        break;
    }
  }

  /** The listbox is one element with one set of handlers, so pointer and keyboard agree. */
  function indexOfTarget(event: Event) {
    const option = (event.target as HTMLElement).closest<HTMLElement>('[data-index]');
    return option ? Number(option.dataset.index) : null;
  }

  function onListClick(event: MouseEvent) {
    const index = indexOfTarget(event);
    if (index !== null) commit(index);
  }

  function onListHover(event: MouseEvent) {
    const index = indexOfTarget(event);
    if (index !== null && !options[index]?.disabled) active = index;
  }

  $effect(() => {
    if (open) list?.focus();
  });
</script>

<div class="select">
  <button
    bind:this={trigger}
    {id}
    type="button"
    class:invalid
    {disabled}
    role="combobox"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={`${id}-listbox`}
    aria-label={label}
    aria-describedby={describedby}
    aria-invalid={invalid ? 'true' : undefined}
    onclick={() => (open ? close() : show())}
    onkeydown={onTriggerKeydown}
  >
    <span class:placeholder={!selected}>{selected?.label ?? placeholder}</span>
    <Icon name="down" size={14} stroke={1.6} />
  </button>

  {#if open}
    <ul
      bind:this={list}
      id={`${id}-listbox`}
      role="listbox"
      tabindex="-1"
      aria-label={label}
      aria-activedescendant={`${id}-option-${active}`}
      onkeydown={onListKeydown}
      onclick={onListClick}
      onmousemove={onListHover}
      use:dismissable={{ ondismiss: () => close(false) }}
    >
      {#each options as option, index (option.value)}
        <li
          id={`${id}-option-${index}`}
          role="option"
          data-index={index}
          aria-selected={option.value === value}
          aria-disabled={option.disabled || undefined}
          class:active={index === active}
        >
          {option.label}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .select {
    position: relative;
  }

  button {
    width: 100%;
    min-height: var(--ui-target-min);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ui-stack);
    padding: var(--c-text-field-padding-block) var(--ui-gutter-inset);
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    font-family: var(--type-body-strong-family);
    font-size: var(--type-body-strong-size);
    font-weight: var(--type-body-strong-weight);
    letter-spacing: var(--type-body-strong-track);
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  button:disabled {
    color: var(--text-muted);
    cursor: not-allowed;
  }

  .invalid {
    border-color: var(--status-error);
  }

  .placeholder {
    color: var(--text-faint);
  }

  ul {
    position: absolute;
    left: 0;
    right: 0;
    z-index: var(--z-tooltip);
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: var(--c-select-list-max-height);
    overflow-y: auto;
    background: var(--surface-raised);
    border: var(--ui-border-hairline) solid var(--border-control);
    box-shadow: 0 var(--ui-stack) var(--c-select-shadow-blur) var(--shadow-overlay);
  }

  ul:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  li {
    min-height: var(--ui-target-min);
    display: flex;
    align-items: center;
    padding: var(--c-text-field-padding-block) var(--ui-gutter-inset);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
    cursor: pointer;
    font-family: var(--type-body-family);
    font-size: var(--type-body-size);
    letter-spacing: var(--type-body-track);
  }

  li:last-child {
    border-bottom: 0;
  }

  .active {
    background: var(--surface-hover);
  }

  li[aria-selected='true'] {
    font-weight: var(--type-body-strong-weight);
  }

  li[aria-disabled='true'] {
    color: var(--text-faint);
    cursor: not-allowed;
  }
</style>
