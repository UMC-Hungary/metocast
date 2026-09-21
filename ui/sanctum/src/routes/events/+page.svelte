<script lang="ts">
  import { onMount } from 'svelte';
  import { _, locale } from 'svelte-i18n';
  import {
    PageHeader,
    IconButton,
    Icon,
    SectionLabel,
    List,
    Row,
    DateBlock,
    Dot,
    Badge,
    Skeleton,
    EmptyState,
    ErrorState,
    Button,
  } from '@metocast/design-system';
  import { listEvents, getEvent } from '@metocast/core-client';
  import type { EventSummary, Event } from '@metocast/core-client/schemas/event';
  import { live } from '$lib/live.svelte';
  import { goto } from '$app/navigation';
  import { monthAbbr, dayNum, timeShort, eventTitle } from '$lib/format';
  import NotifBell from '$lib/NotifBell.svelte';

  type Filter = 'upcoming' | 'live' | 'past';
  const filters: Filter[] = ['upcoming', 'live', 'past'];
  const notificationPreview =
    import.meta.env.DEV && import.meta.env.VITE_SANCTUM_UX_PREVIEW === 'connector-notifications';

  let events = $state<EventSummary[]>([]);
  let phase = $state<'loading' | 'ready' | 'error'>('loading');
  let filter = $state<Filter>('upcoming');
  let query = $state('');
  let detail = $state<Event | null>(null);

  const loc = $derived($locale ?? 'en');

  // ponytail: an event is "live" only when OBS is streaming and it is the current one —
  // the soonest event whose start has passed. No stored live flag exists to read.
  const currentId = $derived(
    live.streaming
      ? events
          .filter((e) => new Date(e.dateTime).getTime() <= Date.now() && !e.isCompleted)
          .sort((a, b) => b.dateTime.localeCompare(a.dateTime))[0]?.id
      : undefined,
  );

  function inFilter(e: EventSummary): boolean {
    const future = new Date(e.dateTime).getTime() >= Date.now();
    if (filter === 'live') return e.id === currentId;
    if (filter === 'past') return e.isCompleted || !future;
    return future && !e.isCompleted;
  }

  const visible = $derived(
    events
      .filter(inFilter)
      .filter((e) => {
        const q = query.trim().toLowerCase();
        return !q || eventTitle(e).toLowerCase().includes(q) || e.speaker.toLowerCase().includes(q);
      })
      .sort((a, b) => a.dateTime.localeCompare(b.dateTime)),
  );

  const featured = $derived(events.find((e) => e.id === currentId) ?? visible[0]);
  const monthLabel = $derived(featured ? monthAbbr(featured.dateTime, loc) : '');

  $effect(() => {
    if (notificationPreview) {
      detail = null;
      return;
    }
    const id = featured?.id;
    if (!id) {
      detail = null;
      return;
    }
    let cancelled = false;
    getEvent(id)
      .then((e) => !cancelled && (detail = e))
      .catch(() => !cancelled && (detail = null));
    return () => (cancelled = true);
  });

  const destinations = $derived(
    detail ? detail.connections.map((c) => c.platform).join(' · ') : '',
  );

  async function load() {
    phase = 'loading';
    try {
      events = await listEvents();
      phase = 'ready';
    } catch {
      phase = 'error';
    }
  }

  function loadPreview(): void {
    const dateTime = new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString();
    events = [
      {
        id: '8f671e76-4f38-4f38-9a0d-3d861fdbd8aa',
        title: 'Sunday Worship Service',
        computedTitle: 'Sunday Worship Service',
        dateTime,
        speaker: 'Rev. Anna Kovács',
        recordingCount: 0,
        isCompleted: false,
        createdAt: dateTime,
        updatedAt: dateTime,
      },
    ];
    phase = 'ready';
  }

  onMount(notificationPreview ? loadPreview : load);
</script>

<PageHeader
  eyebrow={$_('events.eyebrow', { values: { n: visible.length } })}
  title={$_('events.title')}
>
  {#snippet trailing()}
    <NotifBell />
    <IconButton icon="plus" label={$_('events.new')} onclick={() => goto('/events/new')} />
  {/snippet}
</PageHeader>

<div class="events-workspace">
  <div class="schedule-col">
    <section class="search">
      <div>
        <Icon name="search" size={16} />
        <input
          type="search"
          bind:value={query}
          placeholder={$_('events.searchPlaceholder')}
          aria-label={$_('events.search')}
        />
      </div>
    </section>

    <nav class="filters" aria-label={$_('events.filtersLabel')}>
      {#each filters as f (f)}
        <button
          class:active={filter === f}
          aria-pressed={filter === f}
          onclick={() => (filter = f)}
        >
          {$_(`events.filter.${f}`)}
        </button>
      {/each}
    </nav>

    {#if phase === 'loading'}
      <div class="pad"><Skeleton height="56px" lines={3} /></div>
    {:else if phase === 'error'}
      <ErrorState
        title={$_('common.errorTitle')}
        body={$_('common.errorBody')}
        retryLabel={$_('common.retry')}
        onretry={load}
      />
    {:else if visible.length === 0}
      <EmptyState
        title={query ? $_('events.noMatch') : $_('events.emptyTitle')}
        hint={query ? $_('events.noMatchHint') : $_('events.emptyHint')}
      />
    {:else}
      <List>
        {#each visible as e, i (e.id)}
          <Row
            meta={`${timeShort(e.dateTime, loc)} · ${e.speaker || $_('events.noSpeaker')}`}
            href={`/events/${e.id}`}
            last={i === visible.length - 1}
          >
            {#snippet icon()}<DateBlock
                month={monthAbbr(e.dateTime, loc)}
                day={dayNum(e.dateTime, loc)}
              />{/snippet}
            <span class="rowtitle">
              {eventTitle(e)}
              {#if e.id === currentId}<Dot color="var(--status-live)" size={6} pulse />{/if}
            </span>
          </Row>
        {/each}
      </List>
    {/if}
  </div>

  <aside>
    <SectionLabel hint={featured?.id === currentId ? $_('events.liveNow') : $_('events.preview')}
      >{$_('events.selected')}</SectionLabel
    >
    {#if featured}
      <article>
        <DateBlock month={monthLabel} day={dayNum(featured.dateTime, loc)} />
        <div>
          <small
            >{timeShort(featured.dateTime, loc)}{destinations ? ` · ${destinations}` : ''}</small
          >
          <h2>{eventTitle(featured)}</h2>
          <p>
            {#if featured.id === currentId}
              <Badge tone="live" dot>{$_('events.status.live')}</Badge>
            {:else if featured.isCompleted}
              <Badge tone="neutral">{$_('events.status.past')}</Badge>
            {:else}
              <Badge tone="ok">{$_('events.status.scheduled')}</Badge>
            {/if}
          </p>
          <Button variant="primary" compact onclick={() => goto(`/events/${featured.id}`)}
            >{$_('events.open')}</Button
          >
        </div>
      </article>
    {:else}
      <EmptyState title={$_('events.noSelection')} />
    {/if}
  </aside>
</div>

<style>
  .search {
    padding: 0 24px 6px;
  }
  .search div {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    border-block: 1px solid color-mix(in srgb, var(--text-primary) 10%, transparent);
    color: var(--text-muted);
  }
  .search input {
    flex: 1;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    font-size: 14px;
    outline: none;
  }
  .filters {
    padding: 14px 24px 4px;
    display: flex;
    gap: 22px;
  }
  .filters button {
    background: transparent;
    border: 0;
    border-bottom: 1.5px solid transparent;
    padding: 4px 0;
    cursor: pointer;
    font-size: 13px;
    color: var(--text-muted);
  }
  .filters .active {
    color: var(--text-primary);
    border-bottom-color: var(--text-primary);
    font-weight: 500;
  }
  .pad {
    padding: 0 24px;
  }
  .rowtitle {
    line-height: 1.24;
  }
  .rowtitle :global(.dot) {
    margin-left: 8px;
    vertical-align: 1px;
  }
  .events-workspace,
  .schedule-col {
    display: contents;
  }
  aside {
    display: none;
  }

  @media (min-width: 760px) {
    .events-workspace {
      display: grid;
      grid-template-columns: minmax(320px, 1fr) minmax(260px, 340px);
      gap: 18px;
      padding: 0 18px 56px;
      align-items: start;
    }
    .schedule-col,
    aside {
      display: block;
      min-width: 0;
    }
    .search,
    .filters {
      padding-inline: 0;
    }
    aside {
      position: sticky;
      top: 18px;
    }
    article {
      border: 1px solid color-mix(in srgb, var(--text-primary) 16%, transparent);
      padding: 18px;
      display: grid;
      grid-template-columns: auto 1fr;
      gap: 16px;
    }
    article small {
      font-family: var(--font-mono);
      font-size: 10px;
      letter-spacing: 1.4px;
      color: var(--text-muted);
      text-transform: uppercase;
    }
    h2 {
      margin: 8px 0 0;
      font-family: var(--font-display);
      font-size: 24px;
      font-weight: 500;
      line-height: 1.16;
      color: var(--text-primary);
    }
    article p {
      margin: 10px 0 14px;
    }
  }
  @media (min-width: 1360px) {
    .events-workspace {
      grid-template-columns: minmax(560px, 1fr) 360px;
      gap: 28px;
      padding-inline: 32px;
    }
  }
</style>
