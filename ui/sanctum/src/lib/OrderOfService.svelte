<script lang="ts">
  import { _ } from 'svelte-i18n';
  import {
    SectionLabel,
    List,
    Row,
    Button,
    IconButton,
    Icon,
    EmptyState,
    Sheet,
  } from '@metocast/design-system';
  import {
    getEvent,
    getEventOrder,
    setEventOrder,
    orderGo,
    orderStep,
  } from '@metocast/core-client';
  import type { OrderItem } from '@metocast/core-client';
  import type { PptFile } from '@metocast/core-client/schemas/ws-messages';
  import { live } from './live.svelte';
  import DeckLibrary from './DeckLibrary.svelte';
  import SwipeReveal from './SwipeReveal.svelte';

  /**
   * An event's Order of Service. `edit` builds it on the event's page; `run` steps through it
   * during the service, where a click shows the step on the output screen.
   */
  interface Props {
    eventId: string;
    mode: 'edit' | 'run';
    /** Shown in the section label in `run` mode, so it's clear which event is running. */
    eventTitle?: string;
  }

  let { eventId, mode, eventTitle = '' }: Props = $props();

  type Step = { key: string; item: OrderItem };
  let steps = $state<Step[]>([]);
  let readings = $state<{ type: string; reference: string }[]>([]);
  /** The step whose title and notes are being typed, if any. */
  let editing = $state<string | null>(null);
  let newTitle = $state('');
  let error = $state('');
  /** The song the deck picker chooses for; `null` adds a new song. */
  let pickingFor = $state<string | null>(null);
  let picking = $state(false);
  let dragged = $state<string | null>(null);
  let saves = 0;

  const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));
  const toSteps = (items: OrderItem[]) =>
    items.map((item) => ({ key: item.id ?? crypto.randomUUID(), item }));

  async function load() {
    steps = toSteps(await getEventOrder(eventId));
  }

  $effect(() => {
    void live.orderRevision;
    // A change from elsewhere mustn't throw away a rename being typed.
    if (!editing) load().catch((cause) => (error = message(cause)));
  });

  $effect(() => {
    void live.eventsRevision;
    if (mode !== 'edit') return;
    getEvent(eventId).then(
      (event) =>
        (readings = event.bibleReferences.map(({ type, reference }) => ({ type, reference }))),
    );
  });

  const currentIndex = $derived(
    live.orderPosition?.eventId === eventId
      ? steps.findIndex((step) => step.item.id === live.orderPosition?.itemId)
      : -1,
  );

  function detail(item: OrderItem): string {
    if (item.kind === 'song')
      return (
        (item.deckPath ?? '')
          .split(/[/\\]/)
          .pop()
          ?.replace(/\.pptx?$/i, '') ?? ''
      );
    if (item.kind === 'bible')
      return (
        readings.find((reading) => reading.type === item.bibleType)?.reference ??
        $_('order.readingNotSet')
      );
    if (item.kind === 'announcements') {
      const n = bullets(item).length;
      return n ? $_('order.announcementCount', { values: { n } }) : $_('order.noAnnouncements');
    }
    if (item.kind === 'text') return $_('order.slide');
    return '';
  }

  /** The announcements' bullet points: the notes' non-empty lines. */
  const bullets = (item: OrderItem) =>
    item.notes
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean);

  /** Whether the notes are what the step shows (bullets or paragraphs), not notes for the leaders. */
  const notesAreSlides = (item: OrderItem) => item.kind === 'announcements' || item.kind === 'text';

  /** Notes for the leaders; the announcements' and a slide's notes are what they show. */
  const leaderNotes = (item: OrderItem) => (notesAreSlides(item) ? '' : item.notes);

  /**
   * Sends the whole list after every edit. Only the latest save's answer is kept, so a slow
   * reply can't undo a newer edit; a refused list is reloaded from the server.
   */
  async function save() {
    const attempt = ++saves;
    error = '';
    try {
      const stored = await setEventOrder(
        eventId,
        steps.map((step) => step.item),
      );
      if (attempt === saves && !editing) steps = toSteps(stored);
    } catch (cause) {
      error = message(cause);
      await load();
    }
  }

  /**
   * Adds a step named by the text field. A block needs a name; announcements and a slide get one
   * if it's empty and open for editing, since what they show is typed in them.
   */
  function add(kind: 'block' | 'announcements' | 'text') {
    const fallback = {
      block: '',
      announcements: $_('order.kind.announcements'),
      text: $_('order.welcome'),
    };
    const title = newTitle.trim() || fallback[kind];
    if (!title) return;
    const key = crypto.randomUUID();
    steps.push({ key, item: { kind, title, notes: '' } });
    newTitle = '';
    if (kind === 'block') void save();
    else startEditing(key);
  }

  function pickDeck(key: string | null) {
    pickingFor = key;
    picking = true;
  }

  /** A song placeholder takes the deck and its name; with no step, the deck is a new song. */
  function addSong(file: PptFile) {
    const title = file.name.replace(/\.pptx?$/i, '');
    const step = steps.find((candidate) => candidate.key === pickingFor);
    if (step) {
      step.item.deckPath = file.path;
      step.item.title = title;
    } else {
      steps.push({
        key: crypto.randomUUID(),
        item: { kind: 'song', title, deckPath: file.path, notes: '' },
      });
    }
    picking = false;
    void save();
  }

  function move(index: number, to: number) {
    const [step] = steps.splice(index, 1);
    if (step) steps.splice(to, 0, step);
    void save();
  }

  function drop(index: number) {
    const from = steps.findIndex((step) => step.key === dragged);
    dragged = null;
    if (from >= 0 && from !== index) move(from, index);
  }

  function remove(key: string) {
    steps = steps.filter((step) => step.key !== key);
    editing = null;
    void save();
  }

  function startEditing(key: string) {
    if (editing) void save();
    editing = key;
  }

  function stopEditing() {
    if (!editing) return;
    editing = null;
    void save();
  }

  /** Leaving both fields finishes the rename, like Return does. */
  function leave(event: FocusEvent) {
    const next = event.relatedTarget;
    if (!(next instanceof Node && (event.currentTarget as HTMLElement).contains(next)))
      stopEditing();
  }

  /** Puts the cursor in the title as the fields appear. */
  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }
</script>

<section class="order">
  <SectionLabel hint={mode === 'run' ? eventTitle : $_('order.hint')}
    >{$_('order.title')}</SectionLabel
  >

  {#if mode === 'run'}
    {#if steps.length === 0}
      <EmptyState title={$_('order.emptyRun')} hint={$_('order.emptyRunHint')} />
    {:else}
      <List as="ul" aria-label={$_('order.title')}>
        {#each steps as step, index (step.key)}
          <Row
            title={`${index + 1}. ${step.item.title || detail(step.item)}`}
            meta={[step.item.title ? detail(step.item) : '', leaderNotes(step.item)]
              .filter(Boolean)
              .join(' · ')}
            current={index === currentIndex ? 'step' : false}
            last={index === steps.length - 1}
            onclick={() => step.item.id && orderGo(eventId, step.item.id)}
          />
        {/each}
      </List>
      <div class="transport">
        <Button disabled={currentIndex <= 0} onclick={() => orderStep('previous', eventId)}
          >{$_('order.previous')}</Button
        >
        <Button
          variant="primary"
          disabled={currentIndex >= steps.length - 1}
          onclick={() => orderStep('next', eventId)}>{$_('order.next')}</Button
        >
      </div>
    {/if}
  {:else}
    <ol class="steps">
      {#each steps as step, index (step.key)}
        <li
          class="step"
          class:dragging={dragged === step.key}
          ondragover={(event) => event.preventDefault()}
          ondrop={(event) => {
            event.preventDefault();
            drop(index);
          }}
        >
          <SwipeReveal onCommit={() => remove(step.key)} commitLabel={$_('order.remove')}>
            <div class="row" class:current={index === currentIndex}>
              <span
                class="grip"
                draggable="true"
                role="presentation"
                ondragstart={(event) => {
                  dragged = step.key;
                  const row = (event.currentTarget as HTMLElement).closest('li');
                  if (row && event.dataTransfer) {
                    event.dataTransfer.effectAllowed = 'move';
                    event.dataTransfer.setDragImage(row, 16, 16);
                  }
                }}
                ondragend={() => (dragged = null)}
              >
                <Icon name="grip" size={18} stroke={2.6} />
              </span>
              <span class="number">{index + 1}</span>
              {#if editing === step.key}
                <form
                  class="fields"
                  onsubmit={(event) => {
                    event.preventDefault();
                    stopEditing();
                  }}
                  onfocusout={leave}
                >
                  <input
                    class="title"
                    bind:value={step.item.title}
                    use:focusOnMount
                    placeholder={$_('order.titlePlaceholder')}
                    aria-label={$_('order.stepTitle', { values: { n: index + 1 } })}
                  />
                  {#if notesAreSlides(step.item)}
                    <textarea
                      class="notes bullets"
                      bind:value={step.item.notes}
                      rows="5"
                      placeholder={step.item.kind === 'text'
                        ? $_('order.paragraphsPlaceholder')
                        : $_('order.bulletsPlaceholder')}
                      aria-label={step.item.kind === 'text'
                        ? $_('order.stepParagraphs', { values: { n: index + 1 } })
                        : $_('order.stepBullets', { values: { n: index + 1 } })}
                    ></textarea>
                  {:else}
                    <input
                      class="notes"
                      bind:value={step.item.notes}
                      placeholder={$_('order.notesPlaceholder')}
                      aria-label={$_('order.stepNotes', { values: { n: index + 1 } })}
                    />
                  {/if}
                  <div class="edit-actions">
                    <Button compact type="submit">{$_('order.done')}</Button>
                    <Button compact variant="danger" onclick={() => remove(step.key)}
                      >{$_('order.remove')}</Button
                    >
                  </div>
                </form>
              {:else}
                <div
                  class="fields"
                  role="button"
                  tabindex="-1"
                  ondblclick={() => startEditing(step.key)}
                >
                  <span class="name">{step.item.title || detail(step.item)}</span>
                  <span class="kind"
                    >{$_(`order.kind.${step.item.kind}`)}{step.item.title && detail(step.item)
                      ? ` · ${detail(step.item)}`
                      : ''}{leaderNotes(step.item) ? ` · ${leaderNotes(step.item)}` : ''}</span
                  >
                </div>
              {/if}
              <div class="controls">
                {#if step.item.kind === 'song'}
                  <IconButton
                    icon="slides"
                    label={step.item.deckPath ? $_('order.changeDeck') : $_('order.chooseDeck')}
                    onclick={() => pickDeck(step.key)}
                  />
                {/if}
                {#if editing !== step.key}
                  <IconButton
                    icon="edit"
                    label={$_('order.rename', { values: { n: index + 1 } })}
                    onclick={() => startEditing(step.key)}
                  />
                {/if}
                <IconButton
                  icon="up"
                  label={$_('order.moveUp')}
                  disabled={index === 0}
                  onclick={() => move(index, index - 1)}
                />
                <IconButton
                  icon="down"
                  label={$_('order.moveDown')}
                  disabled={index === steps.length - 1}
                  onclick={() => move(index, index + 1)}
                />
              </div>
            </div>
          </SwipeReveal>
        </li>
      {/each}
    </ol>
    <form
      class="add"
      onsubmit={(event) => {
        event.preventDefault();
        add('block');
      }}
    >
      <input
        class="title"
        bind:value={newTitle}
        placeholder={$_('order.addStep')}
        aria-label={$_('order.addStep')}
      />
      <Button compact onclick={() => pickDeck(null)}>{$_('order.addSong')}</Button>
      <Button compact onclick={() => add('announcements')}>{$_('order.addAnnouncements')}</Button>
      <Button compact onclick={() => add('text')}>{$_('order.addSlide')}</Button>
    </form>
    <p class="hint">{$_('order.editHint')}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  {/if}
</section>

<Sheet bind:open={picking} title={$_('order.chooseSong')} eyebrow={$_('order.title')}>
  <div class="picker">
    <DeckLibrary onpick={addSong} />
  </div>
</Sheet>

<style>
  .order {
    display: grid;
    gap: 12px;
  }
  .transport,
  .add {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: space-between;
  }
  .add {
    justify-content: flex-start;
  }
  .steps {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .step {
    overflow: hidden;
    border: var(--border-1) solid var(--border-hairline);
    border-radius: var(--radius-8);
  }
  .step.dragging {
    opacity: 0.5;
  }
  .row {
    display: grid;
    grid-template-columns: auto 24px 1fr auto;
    gap: 10px;
    align-items: center;
    padding: 10px 12px;
  }
  .row.current {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .grip {
    color: var(--text-muted);
    cursor: grab;
  }
  .number {
    color: var(--text-muted);
    font-family: var(--font-mono);
    text-align: right;
  }
  .fields {
    display: grid;
    gap: 4px;
    min-width: 0;
  }
  .name {
    color: var(--text-primary);
  }
  .kind {
    color: var(--text-muted);
    font-family: var(--font-label);
    font-size: var(--type-body-sm-size);
  }
  .title,
  .notes {
    min-height: 36px;
    padding: 0 10px;
    border: var(--border-1) solid var(--border-control);
    border-radius: var(--radius-4);
    background: var(--surface-base);
    color: var(--text-primary);
    font: inherit;
  }
  .notes {
    font-size: var(--type-body-sm-size);
  }
  .bullets {
    padding: 8px 10px;
    resize: vertical;
  }
  .edit-actions {
    display: flex;
    gap: 8px;
  }
  .add .title {
    flex: 1;
  }
  .controls {
    display: flex;
    gap: 2px;
  }
  .hint {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--type-body-sm-size);
  }
  .error {
    margin: 0;
    color: var(--status-error);
    font-size: var(--type-body-sm-size);
  }
  .picker {
    padding: 8px 24px 24px;
  }
</style>
