<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { _, locale } from 'svelte-i18n';
  import {
    PageHeader,
    FormSection,
    TextArea,
    TextField,
    NativeDateInput,
    ReferenceInput,
    Segmented,
    ToggleRow,
    StickyActionBar,
    Skeleton,
    List,
    Row,
    StatusDot,
    Badge,
    Button,
  } from '@metocast/design-system';
  import type { ReferenceResult } from '@metocast/design-system';
  import {
    getEvent,
    listEvents,
    createEvent,
    updateEvent,
    createEventSlides,
    bibleApi,
    listJobs,
    openExternal,
    getTitleTemplate,
    listRecordings,
    renderTitle,
    DEFAULT_TITLE_TEMPLATE,
  } from '@metocast/core-client';
  import type { Event, BibleVerse, EventConnection } from '@metocast/core-client/schemas/event';
  import type { Job } from '@metocast/core-client/schemas/queue';
  import type { Recording } from '@metocast/core-client/schemas/recording';
  import { live } from '$lib/live.svelte';
  import { goto } from '$app/navigation';
  import { notify } from '$lib/notifications.svelte';
  import { dateLong, toDateInput, toTimeInput, fromDateTimeInput } from '$lib/format';
  import NotifBell from '$lib/NotifBell.svelte';
  import TitlePreview from '$lib/TitlePreview.svelte';

  let { eventId }: { eventId?: string } = $props();

  const TITLE_LIMIT = 100;
  const DESC_LIMIT = 5000;
  const TRANSLATION = 'RUF_v2';

  interface RefData {
    ref: string;
    invalid?: boolean;
    translation?: string;
    verses: BibleVerse[];
  }

  let loading = $state(false);
  let previewHeight = $state(0);
  let title = $state('');
  let date = $state('');
  let time = $state('');
  let speaker = $state('');
  let description = $state('');
  let privacy = $state<'public' | 'unlisted' | 'private'>('public');
  let autoUpload = $state(true);
  let textus = $state('');
  let leckio = $state('');
  let textusData = $state<RefData | null>(null);
  let leckioData = $state<RefData | null>(null);
  let existingConnections = $state<Event['connections']>([]);
  let template = $state(DEFAULT_TITLE_TEMPLATE);
  let recordings = $state<Recording[]>([]);
  let pastSpeakers = $state<string[]>([]);

  const speakers = $derived(
    pastSpeakers
      .filter((s, i) => pastSpeakers.indexOf(s) === i)
      .sort(
        (a, b) =>
          pastSpeakers.filter((s) => s === b).length - pastSpeakers.filter((s) => s === a).length,
      ),
  );

  const loc = $derived($locale ?? 'en');

  // Hard cap the title as the reference does, without dropping what the user typed elsewhere.
  $effect(() => {
    if (title.length > TITLE_LIMIT) title = title.slice(0, TITLE_LIMIT);
  });

  const autoTitle = $derived(
    renderTitle(
      template,
      {
        date: date ? new Date(fromDateTimeInput(date, time)) : null,
        title,
        textus,
        leckio,
        speaker,
      },
      loc,
    ),
  );
  const titleRemaining = $derived(TITLE_LIMIT - autoTitle.length);
  const fields = $derived([
    { key: 'date', on: !!date },
    { key: 'textus', on: !!textus.trim() },
    { key: 'title', on: !!title.trim() },
    { key: 'speaker', on: !!speaker.trim() },
  ]);

  function toResult(data: RefData | null): ReferenceResult | null {
    if (!data) return null;
    if (data.invalid) return { invalid: true };
    return {
      verses: data.verses.map((v) => ({ n: `${v.chapter}:${v.verse}`, text: v.text })),
      translation: data.translation,
    };
  }
  const textusResult = $derived(toResult(textusData));
  const leckioResult = $derived(toResult(leckioData));

  function statusMessage(data: RefData | null): string {
    if (!data || data.invalid || data.verses.length === 0) return '';
    return $_('editor.verseCount', { values: { n: data.verses.length, t: data.translation } });
  }

  // A debounced lookup that ignores its own stale responses, so a slower earlier
  // request can never overwrite a newer one (task 7.6).
  function makeLookup(set: (d: RefData | null) => void) {
    let seq = 0;
    let timer: ReturnType<typeof setTimeout>;
    return (ref: string) => {
      clearTimeout(timer);
      const q = ref.trim();
      if (!q) {
        set(null);
        return;
      }
      const my = ++seq;
      timer = setTimeout(async () => {
        try {
          const { verses } = await bibleApi.fetchVerses(q, TRANSLATION);
          if (my !== seq) return;
          set(
            verses.length
              ? { ref: q, translation: TRANSLATION, verses }
              : { ref: q, invalid: true, verses: [] },
          );
        } catch {
          if (my !== seq) return;
          set({ ref: q, invalid: true, verses: [] });
        }
      }, 500);
    };
  }
  const lookupTextus = makeLookup((d) => (textusData = d));
  const lookupLeckio = makeLookup((d) => (leckioData = d));

  $effect(() => lookupTextus(textus));
  $effect(() => lookupLeckio(leckio));

  const privacyOptions = $derived([
    {
      value: 'public' as const,
      label: $_('editor.privacy.public'),
      glyph: '◉',
      hint: $_('editor.privacy.publicHint'),
    },
    {
      value: 'unlisted' as const,
      label: $_('editor.privacy.unlisted'),
      glyph: '◐',
      hint: $_('editor.privacy.unlistedHint'),
    },
    {
      value: 'private' as const,
      label: $_('editor.privacy.private'),
      glyph: '○',
      hint: $_('editor.privacy.privateHint'),
    },
  ]);

  function nextService(): Date {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- plain date arithmetic, not reactive state
    const service = new Date();
    service.setHours(10, 0, 0, 0);
    const now = new Date();
    while (service.getDay() !== 0 || service <= now) service.setDate(service.getDate() + 1);
    return service;
  }

  onMount(async () => {
    listEvents()
      .then((events) => (pastSpeakers = events.map((e) => e.speaker.trim()).filter(Boolean)))
      .catch(() => {});
    template = (await getTitleTemplate().catch(() => null))?.template || DEFAULT_TITLE_TEMPLATE;
    if (!eventId) {
      const service = nextService();
      title = get(_)('editor.defaultTitle');
      date = service.toLocaleDateString('en-CA');
      time = toTimeInput(service.toISOString());
      return;
    }
    loading = true;
    try {
      const [e, eventRecordings] = await Promise.all([getEvent(eventId), listRecordings(eventId)]);
      recordings = eventRecordings;
      title = e.title;
      date = toDateInput(e.dateTime);
      time = toTimeInput(e.dateTime);
      speaker = e.speaker;
      description = e.description;
      autoUpload = e.autoUploadEnabled;
      existingConnections = e.connections;
      jobs = await listJobs('platform_sync').catch(() => []);
      const yt = e.connections.find((c) => c.platform === 'youtube');
      if (
        yt?.privacyStatus === 'unlisted' ||
        yt?.privacyStatus === 'private' ||
        yt?.privacyStatus === 'public'
      )
        privacy = yt.privacyStatus;
      const t = e.bibleReferences.find((r) => r.type === 'textus');
      const l = e.bibleReferences.find((r) => r.type === 'leckio');
      if (t) {
        textus = t.reference;
        textusData = { ref: t.reference, translation: t.translation, verses: t.verses };
      }
      if (l) {
        leckio = l.reference;
        leckioData = { ref: l.reference, translation: l.translation, verses: l.verses };
      }
    } catch {
      notify({ tier: 'error', kind: 'Event', source: 'Core', title: $_('editor.loadFailed') });
    } finally {
      loading = false;
    }
  });

  const PLATFORMS = [
    {
      id: 'youtube',
      label: 'YouTube',
      link: (c: EventConnection) =>
        c.externalId
          ? `https://studio.youtube.com/video/${c.externalId}/livestreaming`
          : c.streamUrl,
    },
    { id: 'facebook', label: 'Facebook', link: (c: EventConnection) => c.eventUrl },
  ] as const;

  const CONNECTOR_TONE = {
    connected: 'ok',
    connecting: 'warn',
    error: 'error',
    disconnected: 'off',
  } as const;

  const STATUS_TONE = {
    not_scheduled: 'off',
    scheduled: 'ok',
    ready: 'ok',
    created: 'warn',
    testing: 'warn',
    live: 'live',
    liveStarting: 'live',
    complete: 'neutral',
    revoked: 'neutral',
    failed: 'error',
  } as const;

  let jobs = $state<Job[]>([]);

  const statusTone = (s: string) => STATUS_TONE[s as keyof typeof STATUS_TONE];
  // Anything YouTube's lifeCycleStatus adds later shows through raw rather than as a missing key.
  const statusLabel = (s: string) => (statusTone(s) ? $_(`editor.platform.status.${s}`) : s);

  const platforms = $derived(
    PLATFORMS.map((p) => {
      const conn = existingConnections.find((c) => c.platform === p.id);
      return {
        id: p.id,
        label: p.label,
        connector: live.connectorStatus[p.id] ?? 'disconnected',
        status: conn?.scheduleStatus ?? 'not_scheduled',
        url: conn ? p.link(conn) : null,
      };
    }),
  );

  // The queue is the only thing that knows an edit has not reached YouTube yet:
  // schedule_status still reads 'scheduled' while the push is outstanding.
  const sync = $derived.by(() => {
    const mine = jobs.filter((j) => j.payload.event_id === eventId);
    const dead = mine.find((j) => j.status === 'dead');
    if (dead) return { tone: 'error' as const, key: 'failed', detail: dead.lastError ?? '' };
    const queued = mine.find((j) => j.status === 'pending' || j.status === 'processing');
    if (queued)
      return live.connectorStatus.youtube === 'connected'
        ? { tone: 'warn' as const, key: 'syncing', detail: '' }
        : { tone: 'off' as const, key: 'parked', detail: '' };
    const yt = existingConnections.find((c) => c.platform === 'youtube');
    if (yt?.externalId) return { tone: 'ok' as const, key: 'synced', detail: '' };
    return { tone: 'warn' as const, key: 'notScheduled', detail: '' };
  });

  function buildRefs() {
    const out: {
      type: string;
      reference: string;
      translation?: string;
      verses?: { chapter: number; verse: number; text: string }[];
    }[] = [];
    if (textusData && !textusData.invalid && textus.trim())
      out.push({
        type: 'textus',
        reference: textus.trim(),
        translation: textusData.translation,
        verses: textusData.verses.map((v) => ({
          chapter: v.chapter,
          verse: v.verse,
          text: v.text,
        })),
      });
    if (leckioData && !leckioData.invalid && leckio.trim())
      out.push({
        type: 'leckio',
        reference: leckio.trim(),
        translation: leckioData.translation,
        verses: leckioData.verses.map((v) => ({
          chapter: v.chapter,
          verse: v.verse,
          text: v.text,
        })),
      });
    return out;
  }

  function duration(seconds: number): string {
    const minutes = Math.floor(seconds / 60);
    return `${minutes}:${Math.floor(seconds % 60)
      .toString()
      .padStart(2, '0')}`;
  }

  function buildConnections() {
    const others = existingConnections.filter((c) => c.platform !== 'youtube');
    const conns = others.map((c) => ({
      platform: c.platform,
      privacy_status: c.privacyStatus ?? undefined,
    }));
    conns.push({ platform: 'youtube', privacy_status: privacy });
    return conns;
  }

  let saving = $state(false);
  const canSave = $derived(title.trim().length > 0 && !!date && !saving);

  // The core writes the decks from what is *stored*, so this only makes sense on
  // a saved event — an unsaved edit would generate the previous references.
  let generating = $state(false);
  const canGenerate = $derived(!!eventId && buildRefs().length > 0 && !generating);

  async function generateSlides() {
    if (!canGenerate || !eventId) return;
    generating = true;
    try {
      const { files } = await createEventSlides(eventId);
      notify({
        tier: 'ok',
        kind: 'Slides',
        source: 'Core',
        title: $_('editor.slides.created'),
        body: files.join('\n'),
        mono: true,
      });
    } catch (e) {
      notify({
        tier: 'error',
        kind: 'Slides',
        source: 'Core',
        title: $_('editor.slides.failed'),
        body: e instanceof Error ? e.message : String(e),
      });
    } finally {
      generating = false;
    }
  }

  async function save() {
    if (!canSave) return;
    saving = true;
    const payload = {
      title: title.trim(),
      computed_title: autoTitle,
      date_time: fromDateTimeInput(date, time),
      speaker: speaker.trim(),
      description: description.trim(),
      auto_upload_enabled: autoUpload,
      connections: buildConnections(),
      bible_references: buildRefs(),
    };
    try {
      if (eventId) await updateEvent(eventId, payload);
      else await createEvent(payload);
      goto('/events');
    } catch (e) {
      // Report and keep every entered value (task 7.2).
      notify({
        tier: 'error',
        kind: 'Event',
        source: 'Core',
        title: $_('editor.saveFailed'),
        body: e instanceof Error ? e.message : undefined,
      });
      saving = false;
    }
  }
</script>

<PageHeader
  back={{ label: $_('editor.back'), href: '/events' }}
  title={eventId ? $_('editor.editTitle') : $_('editor.newTitle')}
>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

{#if loading}
  <div class="pad"><Skeleton height="120px" lines={4} /></div>
{:else}
  <div class="sticky-preview" bind:clientHeight={previewHeight}>
    <TitlePreview
      label={$_('editor.previewLabel')}
      text={autoTitle || $_('editor.previewEmpty')}
      count={`${autoTitle.length}/${TITLE_LIMIT}`}
      warn={titleRemaining < 10}
      tags={fields.map((f) => ({ key: f.key, label: $_(`editor.tag.${f.key}`), on: f.on }))}
    />
  </div>

  <form
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
    style="--preview-h: {previewHeight}px"
  >
    <div class="main-col">
      <FormSection number="01" label={$_('editor.details')}>
        <div class="field">
          <TextArea
            bind:value={title}
            rows={3}
            label={$_('editor.eventTitle')}
            placeholder={$_('editor.titlePlaceholder')}
            invalid={titleRemaining < 0}
          />
          <small class:warn={titleRemaining < 10}>{autoTitle.length} / {TITLE_LIMIT}</small>
        </div>
        <div class="two">
          <div class="field">
            <span class="cap">{$_('editor.date')}</span>
            <NativeDateInput
              type="date"
              bind:value={date}
              label={date ? dateLong(fromDateTimeInput(date, time), loc) : $_('editor.pickDate')}
              icon="calendar"
              accessibleName={$_('editor.date')}
            />
          </div>
          <div class="field">
            <span class="cap">{$_('editor.time')}</span>
            <NativeDateInput
              type="time"
              bind:value={time}
              label={time || $_('editor.pickTime')}
              icon="clock"
              accessibleName={$_('editor.time')}
            />
          </div>
        </div>
        <div class="field">
          <span class="cap">{$_('editor.speaker')}</span>
          <TextField
            bind:value={speaker}
            label={$_('editor.speaker')}
            placeholder={$_('editor.speakerPlaceholder')}
            suggestions={speakers}
            acceptHint={$_('editor.speakerAccept')}
          />
        </div>
      </FormSection>

      <FormSection number="02" label={$_('editor.scripture')} hint={$_('editor.scriptureHint')}>
        <ReferenceInput
          bind:value={textus}
          label={$_('editor.textus')}
          rank={$_('editor.primary')}
          placeholder={$_('editor.textusPlaceholder')}
          result={textusResult}
          statusMessage={statusMessage(textusData)}
          errorMessage={$_('editor.refNotFound')}
          notFoundLabel={$_('editor.refNotFound')}
        />
        <ReferenceInput
          bind:value={leckio}
          label={$_('editor.leckio')}
          rank={$_('editor.secondary')}
          placeholder={$_('editor.leckioPlaceholder')}
          result={leckioResult}
          statusMessage={statusMessage(leckioData)}
          errorMessage={$_('editor.refNotFound')}
          notFoundLabel={$_('editor.refNotFound')}
        />
        {#if eventId}
          <div class="slides">
            <Button variant="secondary" compact onclick={generateSlides} disabled={!canGenerate}>
              {generating ? $_('editor.slides.creating') : $_('editor.slides.create')}
            </Button>
          </div>
        {/if}
      </FormSection>

      <FormSection number="03" label={$_('editor.description')} hint={$_('editor.descriptionHint')}>
        <div class="field">
          <TextArea
            bind:value={description}
            rows={5}
            label={$_('editor.description')}
            placeholder={$_('editor.descriptionPlaceholder')}
          />
          <small class:warn={description.length > DESC_LIMIT}
            >{description.length} / {DESC_LIMIT}</small
          >
        </div>
      </FormSection>
    </div>

    <aside class="publish-col">
      <FormSection number="04" label={$_('editor.privacyLabel')}>
        <Segmented
          bind:value={privacy}
          options={privacyOptions}
          label={$_('editor.privacyLabel')}
        />
      </FormSection>

      <FormSection number="05" label={$_('editor.recording')} last={!eventId}>
        <ToggleRow
          label={$_('editor.autoUpload')}
          sub={$_('editor.autoUploadHint')}
          bind:checked={autoUpload}
        />
        {#if eventId && recordings.length}
          <List>
            {#each recordings as recording, index (recording.id)}
              <Row
                title={recording.fileName}
                meta={`${recording.source} · ${recording.mediaKind} · ${recording.filePath}`}
                detail={duration(recording.durationSeconds)}
                chevron={false}
                last={index === recordings.length - 1}
              />
            {/each}
          </List>
        {/if}
      </FormSection>

      {#if eventId}
        <FormSection number="06" label={$_('editor.platform.label')} last>
          <List>
            {#each platforms as p, i (p.id)}
              {@const url = p.url}
              <Row
                title={p.label}
                meta={statusLabel(p.status)}
                detail={url ? $_('editor.platform.open') : undefined}
                chevron={!!url}
                onclick={url ? () => void openExternal(url) : undefined}
                last={i === platforms.length - 1}
              >
                {#snippet icon()}
                  <StatusDot
                    status={CONNECTOR_TONE[p.connector]}
                    label={$_(`editor.platform.connector.${p.connector}`)}
                    labelHidden
                  />
                {/snippet}
              </Row>
            {/each}
          </List>
          <p class="sync">
            <Badge tone={sync.tone} dot>{$_(`editor.platform.sync.${sync.key}`)}</Badge>
            {#if sync.detail}<small>{sync.detail}</small>{/if}
          </p>
        </FormSection>
      {/if}
    </aside>
  </form>

  <p class="note">{$_('editor.footNote')}</p>
  <StickyActionBar
    primary={eventId ? $_('editor.saveEdit') : $_('editor.saveNew')}
    secondary={$_('editor.cancel')}
    primaryDisabled={!canSave}
    onprimary={save}
    onsecondary={() => goto('/events')}
    bottom="0"
  />
{/if}

<style>
  .pad {
    padding: 24px;
  }
  form {
    padding: 0 20px;
  }
  .slides {
    margin-top: 12px;
  }
  .two {
    display: grid;
    grid-template-columns: 1.35fr 1fr;
    gap: 8px;
  }
  .field {
    margin-bottom: 12px;
  }
  .cap {
    display: block;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 1.2px;
    text-transform: uppercase;
    color: var(--text-muted);
    margin-bottom: 6px;
  }
  small {
    display: block;
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-muted);
    letter-spacing: 1.2px;
    text-align: right;
    margin-top: 6px;
  }
  small.warn {
    color: var(--status-warn);
  }
  .sticky-preview {
    position: sticky;
    top: 0;
    z-index: 5;
    margin: 0 20px 20px;
  }
  .note {
    padding: 24px 32px;
    text-align: center;
    color: var(--text-muted);
    font-family: var(--font-display);
    font-style: italic;
    font-size: 13px;
    line-height: 1.5;
  }
  .main-col,
  .publish-col {
    display: contents;
  }
  @media (min-width: 760px) {
    form {
      display: grid;
      grid-template-columns: minmax(340px, 1fr) minmax(280px, 380px);
      gap: 0 22px;
      padding-inline: 24px;
      align-items: start;
    }
    .main-col,
    .publish-col {
      display: block;
      min-width: 0;
    }
    .publish-col {
      position: sticky;
      top: calc(var(--preview-h, 0px) + 18px);
    }
    .note {
      max-width: 620px;
      margin: 0 auto;
    }
    .sticky-preview {
      margin: 0 0 20px;
      padding-inline: 24px;
    }
  }
  @media (min-width: 1360px) {
    form {
      grid-template-columns: minmax(560px, 1fr) 400px;
      gap: 0 32px;
      padding-inline: 32px;
    }
    .sticky-preview {
      padding-inline: 32px;
    }
  }
</style>
