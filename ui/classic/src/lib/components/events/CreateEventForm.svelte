<script lang="ts">
  import { createEvent, updateEvent } from '@metocast/core-client';
  import type { Event } from '@metocast/core-client/schemas/event';
  import { _ } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import { bibleApi } from '@metocast/core-client/utils/bible-api';
  import { debounce } from '@metocast/core-client/utils/debounce';
  import {
    isV2Translation,
    TRANSLATIONS,
    type BibleTranslation,
    type BibleVerse,
  } from '@metocast/core-client/types/bible';
  import type { BibleSuggestion } from '@metocast/core-client/schemas/bible';
  import BibleSuggestions from '$lib/components/events/BibleSuggestions.svelte';

  interface Props {
    initialEvent?: Event;
    oncreated?: (event: Event) => void;
    onupdated?: (event: Event) => void;
  }

  let { initialEvent, oncreated, onupdated }: Props = $props();

  const isEdit = $derived(!!initialEvent);

  function toDatetimeLocalValue(iso: string): string {
    const d = new Date(iso);
    const offset = d.getTimezoneOffset() * 60000;
    return new Date(d.getTime() - offset).toISOString().slice(0, 16);
  }

  function nextSundayAt(hour: number): string {
    const now = new Date();
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- plain date arithmetic, result is a string
    const sunday = new Date(now);
    sunday.setHours(hour, 0, 0, 0);
    while (sunday.getDay() !== 0 || sunday <= now) sunday.setDate(sunday.getDate() + 1);
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${sunday.getFullYear()}-${pad(sunday.getMonth() + 1)}-${pad(sunday.getDate())}T${pad(hour)}:00`;
  }

  // Basic fields
  let title = $state('');
  let dateTime = $state(nextSundayAt(10));
  let speaker = $state('');
  let description = $state('');
  let youtubePrivacyStatus = $state<'public' | 'unlisted' | 'private'>('private');
  let facebookPrivacyStatus = $state<'EVERYONE' | 'FRIENDS' | 'SELF'>('EVERYONE');

  // Bible state
  let textusTranslation = $state<BibleTranslation>('UF_v2');
  let leckioTranslation = $state<BibleTranslation>('UF_v2');
  let textus = $state('');
  let leckio = $state('');
  let textusVerses = $state<BibleVerse[]>([]);
  let leckioVerses = $state<BibleVerse[]>([]);
  let textusQuery = $state('');
  let leckioQuery = $state('');
  let textusLoading = $state(false);
  let leckioLoading = $state(false);

  // Suggestions state
  let suggestions = $state<BibleSuggestion[]>([]);
  let showSuggestions = $state(false);
  let activeSuggestionField = $state<'textus' | 'leckio' | null>(null);

  // Active bible tab
  let activeTab = $state<'textus' | 'leckio'>('textus');

  // Form submission state
  let submitting = $state(false);
  let error = $state('');

  // Initialize form fields from prop before first render
  $effect.pre(() => {
    title = initialEvent?.title ?? get(_)('events.form.defaultTitle');
    dateTime = initialEvent ? toDatetimeLocalValue(initialEvent.dateTime) : nextSundayAt(10);
    speaker = initialEvent?.speaker ?? '';
    description = initialEvent?.description ?? '';
    const textusRef = initialEvent?.bibleReferences?.find((r) => r.type === 'textus');
    const leckioRef = initialEvent?.bibleReferences?.find((r) => r.type === 'leckio');
    textus = textusRef?.reference ?? '';
    leckio = leckioRef?.reference ?? '';
    textusQuery = textusRef?.reference ?? '';
    leckioQuery = leckioRef?.reference ?? '';
    textusTranslation = (textusRef?.translation as BibleTranslation) ?? 'UF_v2';
    leckioTranslation = (leckioRef?.translation as BibleTranslation) ?? 'UF_v2';
    textusVerses = textusRef?.verses?.map((v) => ({ ...v, editing: false })) ?? [];
    leckioVerses = leckioRef?.verses?.map((v) => ({ ...v, editing: false })) ?? [];
    youtubePrivacyStatus =
      (initialEvent?.connections.find((c) => c.platform === 'youtube')
        ?.privacyStatus as typeof youtubePrivacyStatus) ?? 'private';
    facebookPrivacyStatus =
      (initialEvent?.connections.find((c) => c.platform === 'facebook')
        ?.privacyStatus as typeof facebookPrivacyStatus) ?? 'EVERYONE';
  });

  // Calculated title preview
  const MAX_TITLE_LENGTH = 100;
  const calculatedTitle = $derived(() => {
    if (!dateTime) return '';
    const datePart = dateTime.slice(0, 10);
    const [year, month, day] = datePart.split('-');
    let result = `${year}.${month}.${day}. ${title}`;
    const bibleParts: string[] = [];
    if (textus) bibleParts.push(`Textus: ${textus}`);
    if (leckio) bibleParts.push(`Lekció: ${leckio}`);
    if (bibleParts.length > 0) result += ` | ${bibleParts.join(' ')}`;
    if (speaker) result += ` | ${speaker}`;
    return result;
  });

  // Debounced suggestion fetching (legacy translations)
  const debouncedFetchSuggestions = debounce(async (term: string, field: 'textus' | 'leckio') => {
    if (term.length < 2) {
      suggestions = [];
      showSuggestions = false;
      return;
    }
    try {
      const result = await bibleApi.fetchSuggestions(term);
      suggestions = result;
      showSuggestions = result.length > 0;
      activeSuggestionField = field;
    } catch {
      suggestions = [];
      showSuggestions = false;
    }
  }, 300);

  // Debounced V2 fetching
  const debouncedFetchV2 = debounce(
    async (term: string, translation: BibleTranslation, field: 'textus' | 'leckio') => {
      if (term.length < 2) return;
      try {
        const result = await bibleApi.fetchVerses(term, translation);
        if (field === 'textus') {
          textus = result.label;
          textusVerses = result.verses;
          textusLoading = false;
        } else {
          leckio = result.label;
          leckioVerses = result.verses;
          leckioLoading = false;
        }
      } catch {
        if (field === 'textus') textusLoading = false;
        else leckioLoading = false;
      }
    },
    500,
  );

  function handleBibleInput(field: 'textus' | 'leckio', value: string) {
    if (field === 'textus') textusQuery = value;
    else leckioQuery = value;

    const translation = field === 'textus' ? textusTranslation : leckioTranslation;
    if (isV2Translation(translation)) {
      suggestions = [];
      showSuggestions = false;
      debouncedFetchV2(value, translation, field);
    } else {
      debouncedFetchSuggestions(value, field);
    }
  }

  function handleTranslationChange(field: 'textus' | 'leckio', value: BibleTranslation) {
    if (field === 'textus') {
      textusTranslation = value;
      textusVerses = [];
    } else {
      leckioTranslation = value;
      leckioVerses = [];
    }
    const query = field === 'textus' ? textusQuery : leckioQuery;
    if (query.length >= 2) {
      if (isV2Translation(value)) debouncedFetchV2(query, value, field);
      else debouncedFetchSuggestions(query, field);
    }
  }

  async function handleSuggestionSelect(field: 'textus' | 'leckio', suggestion: BibleSuggestion) {
    if (field === 'textus') {
      textusQuery = suggestion.label;
      textusLoading = true;
    } else {
      leckioQuery = suggestion.label;
      leckioLoading = true;
    }
    suggestions = [];
    showSuggestions = false;
    const translation = field === 'textus' ? textusTranslation : leckioTranslation;
    try {
      const result = await bibleApi.fetchVerses(suggestion.link, translation);
      if (field === 'textus') {
        textus = result.label;
        textusVerses = result.verses;
        textusLoading = false;
      } else {
        leckio = result.label;
        leckioVerses = result.verses;
        leckioLoading = false;
      }
    } catch {
      if (field === 'textus') textusLoading = false;
      else leckioLoading = false;
    }
  }

  async function handleSearch(field: 'textus' | 'leckio') {
    const query = field === 'textus' ? textusQuery : leckioQuery;
    const translation = field === 'textus' ? textusTranslation : leckioTranslation;
    if (!query.trim()) return;
    if (field === 'textus') textusLoading = true;
    else leckioLoading = true;
    try {
      const result = await bibleApi.fetchVerses(query, translation);
      if (field === 'textus') {
        textus = result.label;
        textusVerses = result.verses;
        textusLoading = false;
      } else {
        leckio = result.label;
        leckioVerses = result.verses;
        leckioLoading = false;
      }
    } catch (err) {
      if (field === 'textus') textusLoading = false;
      else leckioLoading = false;
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function clearField(field: 'textus' | 'leckio') {
    if (field === 'textus') {
      textus = '';
      textusVerses = [];
      textusQuery = '';
    } else {
      leckio = '';
      leckioVerses = [];
      leckioQuery = '';
    }
  }

  function toggleEditing(field: 'textus' | 'leckio', index: number) {
    if (field === 'textus') {
      textusVerses = textusVerses.map((v, i) => (i === index ? { ...v, editing: !v.editing } : v));
    } else {
      leckioVerses = leckioVerses.map((v, i) => (i === index ? { ...v, editing: !v.editing } : v));
    }
  }

  function handleVerseChange(field: 'textus' | 'leckio', index: number, text: string) {
    if (field === 'textus') {
      textusVerses = textusVerses.map((v, i) => (i === index ? { ...v, text } : v));
    } else {
      leckioVerses = leckioVerses.map((v, i) => (i === index ? { ...v, text } : v));
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    submitting = true;
    error = '';

    const payload = {
      title,
      date_time: new Date(dateTime).toISOString(),
      speaker: speaker || undefined,
      description: description || undefined,
      bible_references: [
        {
          type: 'textus',
          reference: textus || '',
          translation: textusTranslation,
          verses: textusVerses.map(({ chapter, verse, text }) => ({ chapter, verse, text })),
        },
        {
          type: 'leckio',
          reference: leckio || '',
          translation: leckioTranslation,
          verses: leckioVerses.map(({ chapter, verse, text }) => ({ chapter, verse, text })),
        },
      ],
      connections: [
        { platform: 'youtube', privacy_status: youtubePrivacyStatus },
        { platform: 'facebook', privacy_status: facebookPrivacyStatus },
      ],
    };

    try {
      if (isEdit) {
        const updated = await updateEvent(initialEvent!.id, payload);
        onupdated?.(updated);
      } else {
        const created = await createEvent(payload);
        oncreated?.(created);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }
</script>

<form onsubmit={handleSubmit} class="form">
  {#if error}
    <p class="form__error" role="alert">{error}</p>
  {/if}

  <!-- Calculated title preview -->
  <div class="title-preview">
    <div class="title-preview__header">
      <span class="title-preview__label">Preview title</span>
      <span class="title-preview__counter" class:over={calculatedTitle().length > MAX_TITLE_LENGTH}>
        {calculatedTitle().length}/{MAX_TITLE_LENGTH}
      </span>
    </div>
    <div class="title-preview__value" class:over={calculatedTitle().length > MAX_TITLE_LENGTH}>
      {calculatedTitle() || 'Fill in a date and title to see the preview'}
    </div>
  </div>

  <div class="form__cols">
    <!-- Left: basic info -->
    <div class="form__col">
      <section class="form__section">
        <h2 class="form__section-title">Basic Info</h2>

        <div class="form__field">
          <label for="title">Title *</label>
          <input id="title" type="text" bind:value={title} required />
        </div>

        <div class="form__field">
          <label for="date-time">Date &amp; Time *</label>
          <input id="date-time" type="datetime-local" bind:value={dateTime} required />
        </div>

        <div class="form__field">
          <label for="speaker">Speaker</label>
          <input id="speaker" type="text" bind:value={speaker} />
        </div>

        <div class="form__field">
          <label for="description">Description</label>
          <textarea id="description" bind:value={description} rows={3}></textarea>
        </div>

        <div class="form__field">
          <label for="youtube-privacy">YouTube Visibility</label>
          <select id="youtube-privacy" bind:value={youtubePrivacyStatus}>
            <option value="public">Public</option>
            <option value="unlisted">Unlisted</option>
            <option value="private">Private</option>
          </select>
        </div>

        <div class="form__field">
          <label for="facebook-privacy">Facebook Visibility</label>
          <select id="facebook-privacy" bind:value={facebookPrivacyStatus}>
            <option value="EVERYONE">Public</option>
            <option value="FRIENDS">Friends</option>
            <option value="SELF">Only Me</option>
          </select>
        </div>
      </section>
    </div>

    <!-- Right: bible references -->
    <div class="form__col">
      <section class="form__section">
        <h2 class="form__section-title">Bible References</h2>

        <!-- Tabs -->
        <div class="tabs">
          <button
            type="button"
            class="tabs__tab"
            class:active={activeTab === 'textus'}
            onclick={() => (activeTab = 'textus')}
          >
            Textus {#if textus}<span class="tabs__badge">{textus}</span>{/if}
          </button>
          <button
            type="button"
            class="tabs__tab"
            class:active={activeTab === 'leckio'}
            onclick={() => (activeTab = 'leckio')}
          >
            Lekció {#if leckio}<span class="tabs__badge">{leckio}</span>{/if}
          </button>
        </div>

        {#each ['textus', 'leckio'] as const as field (field)}
          {#if activeTab === field}
            {@const loading = field === 'textus' ? textusLoading : leckioLoading}
            {@const query = field === 'textus' ? textusQuery : leckioQuery}
            {@const translation = field === 'textus' ? textusTranslation : leckioTranslation}
            {@const verses = field === 'textus' ? textusVerses : leckioVerses}
            {@const label = field === 'textus' ? textus : leckio}

            <div class="bible-panel">
              <!-- Translation + Search row -->
              <div class="bible-panel__controls">
                <div class="form__field form__field--narrow">
                  <label for="{field}-translation">Translation</label>
                  <select
                    id="{field}-translation"
                    value={translation}
                    onchange={(e) =>
                      handleTranslationChange(field, e.currentTarget.value as BibleTranslation)}
                  >
                    {#each TRANSLATIONS as t (t.code)}
                      <option value={t.code}>{t.name}</option>
                    {/each}
                  </select>
                </div>

                <div class="form__field form__field--grow" style="position: relative;">
                  <label for="{field}-search">Reference</label>
                  <input
                    id="{field}-search"
                    type="text"
                    value={query}
                    oninput={(e) => handleBibleInput(field, e.currentTarget.value)}
                    placeholder="e.g. Jn 3,16-21"
                  />
                  {#if showSuggestions && activeSuggestionField === field}
                    <BibleSuggestions
                      {suggestions}
                      visible={true}
                      onSelect={(s) => handleSuggestionSelect(field, s)}
                    />
                  {/if}
                </div>

                <button
                  type="button"
                  class="bible-panel__fetch-btn"
                  onclick={() => handleSearch(field)}
                  disabled={loading}
                >
                  {loading ? '…' : 'Fetch'}
                </button>

                {#if query || label || verses.length > 0}
                  <button
                    type="button"
                    class="bible-panel__remove-btn"
                    onclick={() => clearField(field)}
                    aria-label="Remove {field} reference"
                  >
                    ✕
                  </button>
                {/if}
              </div>

              <!-- Verse list -->
              {#if verses.length > 0}
                <div class="verses">
                  <div class="verses__header">
                    <span class="verses__label">{label}</span>
                    <button type="button" class="verses__clear" onclick={() => clearField(field)}>
                      Clear
                    </button>
                  </div>
                  {#each verses as verse, index (index)}
                    <div class="verse">
                      <span class="verse__ref">{verse.chapter}:{verse.verse}</span>
                      {#if verse.editing}
                        <textarea
                          class="verse__edit"
                          value={verse.text}
                          rows={2}
                          oninput={(e) => handleVerseChange(field, index, e.currentTarget.value)}
                        ></textarea>
                      {:else}
                        <p class="verse__text">{verse.text}</p>
                      {/if}
                      <button
                        type="button"
                        class="verse__toggle"
                        onclick={() => toggleEditing(field, index)}
                        aria-label={verse.editing ? 'Done editing' : 'Edit verse'}
                      >
                        {verse.editing ? '✓' : '✎'}
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        {/each}
      </section>
    </div>
  </div>

  <div class="form__actions">
    <a href={isEdit ? `/events/${initialEvent?.id}` : '/events'} class="btn-cancel">Cancel</a>
    <button type="submit" disabled={submitting}>
      {#if submitting}
        {isEdit ? 'Saving…' : 'Creating…'}
      {:else}
        {isEdit ? 'Save Changes' : 'Create Event'}
      {/if}
    </button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  .form__error {
    padding: 0.75rem;
    background: var(--status-err-bg);
    color: var(--status-err-text);
    border-radius: 0.375rem;
    font-size: 0.875rem;
  }

  /* Title preview */
  .title-preview {
    padding: 0.875rem 1rem;
    background: var(--content-bg);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
  }

  .title-preview__header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.375rem;
  }

  .title-preview__label {
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-secondary);
  }

  .title-preview__counter {
    font-size: 0.75rem;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }

  .title-preview__counter.over {
    color: var(--status-err-dot);
    font-weight: 600;
  }

  .title-preview__value {
    font-size: 0.875rem;
    color: var(--text-primary);
    line-height: 1.5;
    word-break: break-word;
  }

  .title-preview__value.over {
    color: var(--status-err-dot);
  }

  /* Two-column layout */
  .form__cols {
    display: grid;
    grid-template-columns: 1fr;
    gap: 1.25rem;
  }

  @media (min-width: 768px) {
    .form__cols {
      grid-template-columns: 1fr 1.5fr;
    }
  }

  .form__section {
    padding: 1.25rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .form__section-title {
    margin: 0 0 0.25rem;
    font-size: 1rem;
    font-weight: 600;
  }

  .form__field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .form__field--narrow {
    width: 8rem;
    flex-shrink: 0;
  }

  .form__field--grow {
    flex: 1;
    min-width: 0;
  }

  .form__field label {
    font-size: 0.875rem;
    font-weight: 500;
  }

  .form__field input,
  .form__field select,
  .form__field textarea {
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--input-border);
    border-radius: 0.375rem;
    font-size: 0.9375rem;
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
    background: var(--input-bg);
  }

  .form__field input:focus,
  .form__field select:focus,
  .form__field textarea:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 15%, transparent);
  }

  /* Tabs */
  .tabs {
    display: flex;
    gap: 0.25rem;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0;
    margin-bottom: 1rem;
  }

  .tabs__tab {
    padding: 0.5rem 1rem;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-secondary);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.375rem;
  }

  .tabs__tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .tabs__badge {
    font-size: 0.6875rem;
    padding: 0.1rem 0.375rem;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: 9999px;
    max-width: 8rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Bible panel */
  .bible-panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .bible-panel__controls {
    display: flex;
    gap: 0.5rem;
    align-items: flex-end;
    flex-wrap: wrap;
  }

  .bible-panel__fetch-btn {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    flex-shrink: 0;
    align-self: flex-end;
    height: 2.375rem;
  }

  .bible-panel__fetch-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .bible-panel__fetch-btn:not(:disabled):hover {
    filter: brightness(0.9);
  }

  .bible-panel__remove-btn {
    padding: 0.5rem 0.75rem;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    flex-shrink: 0;
    align-self: flex-end;
    height: 2.375rem;
  }

  .bible-panel__remove-btn:hover {
    background: var(--status-err-bg);
    color: var(--status-err-text);
    border-color: var(--border);
  }

  /* Verses */
  .verses {
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    overflow: hidden;
    font-size: 0.875rem;
  }

  .verses__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.75rem;
    background: var(--content-bg);
    border-bottom: 1px solid var(--border);
  }

  .verses__label {
    font-weight: 600;
    color: var(--text-primary);
  }

  .verses__clear {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 0.25rem;
    padding: 0.125rem 0.5rem;
    font-size: 0.75rem;
    cursor: pointer;
    color: var(--text-secondary);
  }

  .verses__clear:hover {
    background: var(--status-err-bg);
    border-color: var(--border);
    color: var(--status-err-text);
  }

  .verse {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--border);
  }

  .verse:last-child {
    border-bottom: none;
  }

  .verse__ref {
    flex-shrink: 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-secondary);
    padding: 0.125rem 0.375rem;
    background: var(--content-bg);
    border-radius: 0.25rem;
    margin-top: 0.125rem;
  }

  .verse__text {
    flex: 1;
    margin: 0;
    line-height: 1.5;
    color: var(--text-primary);
  }

  .verse__edit {
    flex: 1;
    padding: 0.375rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 0.25rem;
    font-size: 0.875rem;
    font-family: inherit;
    resize: vertical;
    box-sizing: border-box;
  }

  .verse__toggle {
    flex-shrink: 0;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 0.25rem;
    width: 1.75rem;
    height: 1.75rem;
    cursor: pointer;
    font-size: 0.875rem;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-top: 0.125rem;
  }

  .verse__toggle:hover {
    background: var(--nav-item-hover);
  }

  /* Actions */
  .form__actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.75rem;
    padding-top: 0.5rem;
  }

  .btn-cancel {
    padding: 0.625rem 1.25rem;
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 1rem;
    text-decoration: none;
    cursor: pointer;
  }

  .btn-cancel:hover {
    background: var(--nav-item-hover);
  }

  button[type='submit'] {
    padding: 0.625rem 1.25rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    font-size: 1rem;
    cursor: pointer;
  }

  button[type='submit']:not(:disabled):hover {
    filter: brightness(0.9);
  }

  button[type='submit']:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
