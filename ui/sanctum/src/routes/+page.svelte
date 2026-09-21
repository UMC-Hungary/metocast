<script lang="ts">
  import { onMount } from 'svelte';
  import { _, locale } from 'svelte-i18n';
  import {
    PageHeader,
    Lockup,
    Dot,
    Stat,
    SectionLabel,
    List,
    Row,
    DateBlock,
    TextIcon,
    Skeleton,
    ErrorState,
    EmptyState,
    Button,
  } from '@metocast/design-system';
  import {
    fetchConnectorConfig,
    fetchConnectorStatuses,
    listEvents,
    startRodecasterAudioRecording,
    stopRodecasterAudioRecording,
    type RodecasterAudioRecordingConfig,
  } from '@metocast/core-client';
  import type { EventSummary } from '@metocast/core-client/schemas/event';
  import { live } from '$lib/live.svelte';
  import { monthAbbr, dayNum, timeShort, dateTimeLabel, clock, eventTitle } from '$lib/format';
  import NotifBell from '$lib/NotifBell.svelte';

  let events = $state<EventSummary[]>([]);
  let connected = $state(0);
  let phase = $state<'loading' | 'ready' | 'error'>('loading');
  let recordingPending = $state(false);
  let recordingConfig = $state<RodecasterAudioRecordingConfig | null>(null);
  let tick = $state(Date.now());

  const loc = $derived($locale ?? 'en');
  const now = new Date();
  const hour = now.getHours() + now.getMinutes() / 60;
  const greeting =
    hour >= 23 || hour < 4.5
      ? 'greetingNight'
      : hour < 8
        ? 'greetingEarly'
        : hour < 12
          ? 'greetingMorning'
          : hour < 18
            ? 'greetingAfternoon'
            : 'greetingEvening';
  const nextEvent = $derived(
    events
      .filter((e) => new Date(e.dateTime).getTime() >= Date.now())
      .sort((a, b) => a.dateTime.localeCompare(b.dateTime))[0],
  );
  const recorderState = $derived(live.rodecasterAudioRecorder);
  const audioRecording = $derived(recorderState?.status === 'recording');
  const sessionEvent = $derived(events.find((e) => e.id === recorderState?.eventId) ?? null);
  // Idle, the card offers the event the core picked; recording, it names the one the
  // running session is bound to, so a session never appears to move mid-recording.
  const recordTarget = $derived(
    audioRecording ? (sessionEvent ?? live.currentEvent) : live.currentEvent,
  );
  const canRecord = $derived(
    recordTarget !== null &&
      !recordTarget.isCompleted &&
      recordingConfig?.enabled === true &&
      live.connectorStatus.rodecaster === 'connected',
  );
  const elapsed = $derived(
    audioRecording && recorderState?.startedAt
      ? clock((tick - Date.parse(recorderState.startedAt)) / 1000)
      : '—',
  );

  $effect(() => {
    if (recorderState) recordingPending = false;
  });

  $effect(() => {
    if (!audioRecording) return;
    const id = setInterval(() => (tick = Date.now()), 1000);
    return () => clearInterval(id);
  });

  async function load() {
    phase = 'loading';
    try {
      events = await listEvents();
      recordingConfig = (await fetchConnectorConfig('rodecaster')).audioRecording;
      phase = 'ready';
    } catch {
      phase = 'error';
    }
    try {
      const s = await fetchConnectorStatuses();
      connected = Object.values(s).filter((c) => c?.type === 'connected').length;
    } catch {
      /* offline — leave count at 0 */
    }
  }

  onMount(load);

  function sendRecordCommand(): boolean {
    if (audioRecording) return stopRodecasterAudioRecording();
    return recordTarget ? startRodecasterAudioRecording(recordTarget.id) : false;
  }

  function toggleAudioRecording(): void {
    recordingPending = true;
    const sent = sendRecordCommand();
    if (!sent) recordingPending = false;
    else setTimeout(() => (recordingPending = false), 5000);
  }
</script>

<PageHeader title={$_(`dash.${greeting}`)} eyebrowContent={eyebrow}>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>
{#snippet eyebrow()}<Lockup name="Metocast" markSize={16} fontSize={15} tracking={1.6} />{/snippet}

<div class="dashboard-grid">
  <section class="now">
    <article>
      <header>
        <p>
          <Dot
            color={live.streaming ? 'var(--status-live)' : 'var(--text-muted)'}
            size={8}
            pulse={live.streaming}
          />
          <span class:live={live.streaming}
            >{live.streaming ? $_('dash.onAir') : $_('dash.offAir')}</span
          >
        </p>
        <time>{live.streaming ? '00:42:18' : '—'}</time>
      </header>
      <h2>{live.streaming ? $_('dash.activeTitle') : $_('dash.noBroadcast')}</h2>
      <p class="sub">{live.streaming ? $_('dash.activeContext') : $_('dash.idleHint')}</p>
      <div class="stats">
        <Stat label={$_('dash.viewers')} value={live.streaming ? '1,284' : '—'} />
        <Stat
          label={$_('dash.bitrate')}
          value={live.streaming ? '6.2' : '—'}
          unit={live.streaming ? 'Mb/s' : ''}
        />
        <Stat
          label={$_('dash.dropped')}
          value={live.streaming ? '0.00' : '—'}
          unit={live.streaming ? '%' : ''}
        />
      </div>
    </article>
  </section>

  {#if recordingConfig?.enabled}
    <section class="recorder">
      <SectionLabel hint={$_(`dash.outputMode.${recordingConfig.outputMode}`)}
        >{$_('dash.audioRecording')}</SectionLabel
      >
      <article>
        <header>
          <p>
            <Dot
              color={audioRecording ? 'var(--status-live)' : 'var(--text-muted)'}
              size={8}
              pulse={audioRecording}
            />
            <span class:live={audioRecording}
              >{audioRecording ? $_('dash.recActive') : $_('dash.recReady')}</span
            >
          </p>
          <time>{elapsed}</time>
        </header>
        <h2>{recordTarget ? eventTitle(recordTarget) : $_('dash.noRecordEvent')}</h2>
        <p class="sub">
          {recordTarget ? dateTimeLabel(recordTarget.dateTime, loc) : $_('dash.noRecordEventHint')}
        </p>
        <div class="action">
          <Button
            variant={audioRecording ? 'danger' : 'primary'}
            block
            loading={recordingPending}
            disabled={!audioRecording && !canRecord}
            onclick={toggleAudioRecording}
          >
            {audioRecording ? $_('dash.stopAudio') : $_('dash.recordAudio')}
          </Button>
          {#if recorderState?.error}
            <p class="hint error">{recorderState.error}</p>
          {:else if live.connectorStatus.rodecaster !== 'connected'}
            <p class="hint">{$_('dash.connectMixer')}</p>
          {:else if recorderState?.finalizedFiles[0]}
            <p class="hint">{$_('dash.lastRecording')}: {recorderState.finalizedFiles[0].path}</p>
          {/if}
        </div>
      </article>
    </section>
  {/if}

  <div class="main-col">
    <SectionLabel hint={nextEvent ? timeShort(nextEvent.dateTime, loc) : ''}
      >{$_('dash.upNext')}</SectionLabel
    >
    {#if phase === 'loading'}
      <div class="pad"><Skeleton height="64px" /></div>
    {:else if phase === 'error'}
      <ErrorState
        title={$_('common.errorTitle')}
        body={$_('common.errorBody')}
        retryLabel={$_('common.retry')}
        onretry={load}
      />
    {:else if nextEvent}
      <List>
        <Row
          title={eventTitle(nextEvent)}
          meta={`${timeShort(nextEvent.dateTime, loc)} · ${nextEvent.speaker || $_('dash.noSpeaker')}`}
          href={`/events/${nextEvent.id}`}
          last
        >
          {#snippet icon()}<DateBlock
              month={monthAbbr(nextEvent.dateTime, loc)}
              day={dayNum(nextEvent.dateTime, loc)}
            />{/snippet}
        </Row>
      </List>
    {:else}
      <EmptyState title={$_('dash.noneScheduled')} hint={$_('dash.noneScheduledHint')} />
    {/if}
  </div>

  <aside class="side-col">
    <SectionLabel>{$_('dash.quickActions')}</SectionLabel>
    <List>
      <Row title={$_('dash.newEvent')} meta={$_('dash.newEventMeta')} href="/events/new">
        {#snippet icon()}<TextIcon char="+" />{/snippet}
      </Row>
      <Row
        title={$_('dash.presentations')}
        meta={$_('dash.presentationsMeta')}
        href="/presentations"
      >
        {#snippet icon()}<TextIcon char="▥" />{/snippet}
      </Row>
      <Row
        title={$_('dash.connectors')}
        meta={$_('dash.connectorsMeta', { values: { n: connected } })}
        href="/settings/connectors"
        last
      >
        {#snippet icon()}<TextIcon char="↳" />{/snippet}
      </Row>
    </List>
  </aside>
</div>

<style>
  .now {
    padding: 0 24px;
  }
  .pad {
    padding: 0 24px;
  }
  .recorder > article {
    margin: 0 24px;
  }
  .action {
    display: grid;
    gap: 12px;
    margin-top: 22px;
    padding-top: 18px;
    border-top: 1px solid color-mix(in srgb, var(--text-primary) 12%, transparent);
  }
  .hint {
    margin: 0;
    overflow-wrap: anywhere;
    color: var(--text-muted);
    font-size: 12px;
  }
  .hint.error {
    color: var(--status-error);
  }
  article {
    border: 1px solid color-mix(in srgb, var(--text-primary) 16%, transparent);
    padding: 20px 20px 22px;
  }
  header,
  header p {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  header p {
    gap: 9px;
    margin: 0;
  }
  header span,
  time {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 2px;
    text-transform: uppercase;
    color: var(--text-muted);
    font-weight: 500;
  }
  header span.live {
    color: var(--status-live);
  }
  time {
    font-size: 12px;
    letter-spacing: 0.5px;
    text-transform: none;
    color: var(--text-secondary);
  }
  h2 {
    margin: 16px 0 0;
    font-family: var(--font-display);
    font-size: 26px;
    line-height: 1.12;
    color: var(--text-primary);
    font-weight: 500;
  }
  .sub {
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    margin-top: 22px;
    padding-top: 18px;
    border-top: 1px solid color-mix(in srgb, var(--text-primary) 12%, transparent);
  }
  .dashboard-grid,
  .main-col,
  .side-col {
    display: contents;
  }

  @media (min-width: 760px) {
    .dashboard-grid {
      display: grid;
      grid-template-columns: minmax(320px, 1fr) minmax(260px, 340px);
      /* Dense so the quick actions rise to the top of the right column on a core
         with no RØDECaster, where the recorder card is not rendered at all. */
      grid-auto-flow: row dense;
      gap: 18px;
      padding: 0 18px 56px;
      align-items: start;
    }
    .main-col,
    .side-col {
      display: block;
      min-width: 0;
    }
    .now,
    .main-col {
      grid-column: 1;
    }
    .recorder,
    .side-col {
      grid-column: 2;
    }
    .now,
    .pad {
      padding-inline: 0;
    }
    /* Only the gutter margin goes: the card keeps the padding every card has. */
    .recorder > article {
      margin-inline: 0;
    }
    .side-col {
      position: sticky;
      top: 18px;
    }
  }
  @media (min-width: 1360px) {
    .dashboard-grid {
      grid-template-columns: minmax(560px, 1fr) 360px;
      gap: 28px;
      padding-inline: 32px;
    }
  }
</style>
