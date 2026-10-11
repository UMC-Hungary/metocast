<script lang="ts">
  import { _ } from 'svelte-i18n';
  import {
    SectionLabel,
    List,
    Row,
    Button,
    IconButton,
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
  let dirty = $state(false);
  let saving = $state(false);
  let error = $state('');
  let picking = $state(false);

  const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

  async function load() {
    const items = await getEventOrder(eventId);
    steps = items.map((item) => ({ key: item.id ?? crypto.randomUUID(), item }));
  }

  $effect(() => {
    void live.orderRevision;
    // A change from elsewhere mustn't throw away an edit in progress.
    if (!dirty) load().catch((cause) => (error = message(cause)));
  });

  $effect(() => {
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

  function readingLabel(type: string | null | undefined): string {
    return type === 'textus' ? $_('order.textus') : $_('order.leckio');
  }

  function detail(item: OrderItem): string {
    if (item.kind === 'song')
      return (
        (item.deckPath ?? '')
          .split(/[/\\]/)
          .pop()
          ?.replace(/\.pptx?$/i, '') ?? ''
      );
    if (item.kind === 'bible') return readingLabel(item.bibleType);
    return '';
  }

  function add(item: OrderItem) {
    steps.push({ key: crypto.randomUUID(), item });
    dirty = true;
  }

  function addSong(file: PptFile) {
    add({
      kind: 'song',
      title: file.name.replace(/\.pptx?$/i, ''),
      deckPath: file.path,
      notes: '',
    });
    picking = false;
  }

  function move(index: number, by: number) {
    const [step] = steps.splice(index, 1);
    if (step) steps.splice(index + by, 0, step);
    dirty = true;
  }

  function remove(index: number) {
    steps.splice(index, 1);
    dirty = true;
  }

  async function save() {
    saving = true;
    error = '';
    try {
      const stored = await setEventOrder(
        eventId,
        steps.map((step) => step.item),
      );
      steps = stored.map((item) => ({ key: item.id ?? crypto.randomUUID(), item }));
      dirty = false;
    } catch (cause) {
      error = message(cause);
    } finally {
      saving = false;
    }
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
            meta={[step.item.title ? detail(step.item) : '', step.item.notes]
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
    {#if steps.length === 0}
      <EmptyState title={$_('order.empty')} hint={$_('order.emptyHint')} />
    {/if}
    <ol class="steps">
      {#each steps as step, index (step.key)}
        <li class="step">
          <span class="number">{index + 1}</span>
          <div class="fields">
            <span class="kind"
              >{$_(`order.kind.${step.item.kind}`)}{detail(step.item)
                ? ` · ${detail(step.item)}`
                : ''}</span
            >
            <input
              class="title"
              bind:value={step.item.title}
              oninput={() => (dirty = true)}
              placeholder={$_('order.titlePlaceholder')}
              aria-label={$_('order.stepTitle', { values: { n: index + 1 } })}
            />
            <input
              class="notes"
              bind:value={step.item.notes}
              oninput={() => (dirty = true)}
              placeholder={$_('order.notesPlaceholder')}
              aria-label={$_('order.stepNotes', { values: { n: index + 1 } })}
            />
          </div>
          <div class="controls">
            <IconButton
              icon="up"
              label={$_('order.moveUp')}
              disabled={index === 0}
              onclick={() => move(index, -1)}
            />
            <IconButton
              icon="down"
              label={$_('order.moveDown')}
              disabled={index === steps.length - 1}
              onclick={() => move(index, 1)}
            />
            <IconButton icon="close" label={$_('order.remove')} onclick={() => remove(index)} />
          </div>
        </li>
      {/each}
    </ol>
    <div class="add">
      <Button compact onclick={() => (picking = true)}>{$_('order.addSong')}</Button>
      <Button compact onclick={() => add({ kind: 'song', title: '', notes: '' })}
        >{$_('order.addSongNoSlides')}</Button
      >
      {#each readings as reading (reading.type)}
        <Button
          compact
          onclick={() =>
            add({
              kind: 'bible',
              title: readingLabel(reading.type),
              bibleType: reading.type === 'textus' ? 'textus' : 'leckio',
              notes: '',
            })}>{readingLabel(reading.type)} · {reading.reference}</Button
        >
      {/each}
      <Button compact onclick={() => add({ kind: 'block', title: '', notes: '' })}
        >{$_('order.addBlock')}</Button
      >
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <Button
      variant="primary"
      block
      disabled={!dirty}
      loading={saving}
      loadingLabel={$_('order.saving')}
      onclick={save}
    >
      {dirty ? $_('order.save') : $_('order.saved')}
    </Button>
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
    display: grid;
    grid-template-columns: 28px 1fr auto;
    gap: 12px;
    align-items: start;
    padding: 12px;
    border: var(--border-1) solid var(--border-hairline);
    border-radius: var(--radius-8);
    background: var(--surface-raised);
  }
  .number {
    padding-top: 2px;
    color: var(--text-muted);
    font-family: var(--font-mono);
    text-align: right;
  }
  .fields {
    display: grid;
    gap: 6px;
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
  .controls {
    display: flex;
    gap: 2px;
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
