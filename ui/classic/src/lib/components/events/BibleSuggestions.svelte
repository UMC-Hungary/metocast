<script lang="ts">
  import type { BibleSuggestion } from '@metocast/core-client/schemas/bible';

  interface Props {
    suggestions: BibleSuggestion[];
    visible: boolean;
    onSelect: (suggestion: BibleSuggestion) => void;
  }

  let { suggestions, visible, onSelect }: Props = $props();

  function handleKeyDown(event: KeyboardEvent, suggestion: BibleSuggestion) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect(suggestion);
    }
  }
</script>

{#if visible && suggestions.length > 0}
  <div class="suggestions" role="listbox">
    <div class="suggestions__header">Suggestions:</div>
    {#each suggestions as suggestion (suggestion.link)}
      <button
        type="button"
        role="option"
        aria-selected={false}
        tabindex={0}
        class="suggestions__item"
        onclick={() => onSelect(suggestion)}
        onkeydown={(e) => handleKeyDown(e, suggestion)}
      >
        {suggestion.label}
      </button>
    {/each}
  </div>
{/if}

<style>
  .suggestions {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 50;
    width: 100%;
    margin-top: 0.25rem;
    max-height: 15rem;
    overflow-y: auto;
    background: var(--glass-card-bg);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1);
  }

  .suggestions__header {
    padding: 0.375rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border);
  }

  .suggestions__item {
    display: block;
    width: 100%;
    padding: 0.5rem 0.75rem;
    text-align: left;
    font-size: 0.875rem;
    background: transparent;
    border: none;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .suggestions__item:hover,
  .suggestions__item:focus {
    background: var(--nav-item-hover);
    outline: none;
  }
</style>
