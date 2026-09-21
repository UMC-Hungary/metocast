<script lang="ts">
  import { untrack } from 'svelte';
  import { sendWsCommand } from '@metocast/core-client';
  import type { SlideContent } from '@metocast/core-client/schemas/ws-messages';

  interface Props {
    slides: SlideContent[];
    onclose: () => void;
  }

  let { slides, onclose }: Props = $props();

  // Local copy of slide content so changes can be debounced before sending.
  // Each entry is all paragraph texts joined by '\n' for editing in a textarea.
  // untrack prevents Svelte from warning about capturing the initial prop value.
  let localTexts = $state<string[]>(
    untrack(() => slides.map((s) => s.paragraphs.flatMap((p) => p.lines).join('\n'))),
  );

  let debounceTimers = $state<(ReturnType<typeof setTimeout> | null)[]>(
    untrack(() => slides.map(() => null)),
  );

  function handleInput(slideIndex: number, value: string) {
    localTexts[slideIndex] = value;

    const timerIdx = slideIndex;
    if (debounceTimers[timerIdx] !== null) {
      clearTimeout(debounceTimers[timerIdx]!);
    }
    debounceTimers[timerIdx] = setTimeout(() => {
      const slide = slides[slideIndex];
      if (!slide) return;
      const texts = value.split('\n').filter((line) => line.trim() !== '');
      sendWsCommand('presenter.slide.update', {
        slide_index: slide.index,
        texts,
      });
      debounceTimers[timerIdx] = null;
    }, 400);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      onclose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="backdrop"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onclose();
  }}
>
  <div class="modal" role="dialog" aria-modal="true" aria-label="Edit slide content">
    <div class="modal__header">
      <h2>Edit slides</h2>
      <button class="modal__close" onclick={onclose} aria-label="Close">✕</button>
    </div>

    {#if slides.length === 0}
      <p class="empty">No slides loaded.</p>
    {:else}
      <div class="slides-list">
        {#each slides as slide, i (slide.index)}
          <div class="slide-item">
            <div class="slide-label">Slide {slide.index}</div>
            <textarea
              class="slide-textarea"
              rows={4}
              value={localTexts[i]}
              oninput={(e) => handleInput(i, (e.target as HTMLTextAreaElement).value)}
              aria-label="Slide {slide.index} content"
            ></textarea>
          </div>
        {/each}
      </div>
    {/if}

    <div class="modal__footer">
      <button class="btn-secondary" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--modal-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .modal {
    background: var(--modal-card-bg);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    padding: 1.5rem;
    width: min(640px, 92vw);
    max-height: 85vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .modal__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .modal__header h2 {
    margin: 0;
    font-size: 1.125rem;
    font-weight: 600;
  }

  .modal__close {
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 1rem;
    cursor: pointer;
    padding: 0.25rem;
    line-height: 1;
  }

  .modal__close:hover {
    color: var(--text-primary);
  }

  .slides-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .slide-item {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }

  .slide-label {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .slide-textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 0.5rem 0.625rem;
    border: 1px solid var(--input-border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    background: var(--input-bg, transparent);
    color: var(--text-primary);
    resize: vertical;
    font-family: inherit;
    line-height: 1.5;
  }

  .slide-textarea:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }

  .modal__footer {
    display: flex;
    justify-content: flex-end;
  }

  .btn-secondary {
    padding: 0.5rem 1.25rem;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-secondary:hover {
    color: var(--text-primary);
    background: var(--nav-item-hover);
  }

  .empty {
    color: var(--text-secondary);
    font-size: 0.875rem;
  }
</style>
