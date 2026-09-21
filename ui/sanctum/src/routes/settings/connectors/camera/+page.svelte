<script lang="ts">
  import { onMount } from 'svelte';
  import { _, locale } from 'svelte-i18n';
  import {
    PageHeader,
    SectionLabel,
    List,
    Row,
    OverviewCell,
    Icon,
    Segmented,
    Select,
    RadioGroup,
    ProgressBar,
    Stat,
    Spinner,
    TransportDock,
    StickyActionBar,
    ErrorState,
  } from '@metocast/design-system';
  import {
    fetchCameraSettings,
    applyCameraSettings,
    sendWsCommand,
    type CameraSettings,
    type CameraSettingsUpdate,
    type CameraSupportedFormat,
  } from '@metocast/core-client';
  import { live } from '$lib/live.svelte';
  import { pushToast } from '$lib/notifications.svelte';
  import NotifBell from '$lib/NotifBell.svelte';

  type Draft = {
    resolution: string;
    frameRate: string;
    codec: string;
    server: string;
    quality: string;
  };

  let settings = $state<CameraSettings | null>(null);
  let draft = $state<Draft | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let failure = $state('');

  const resolutionKey = (f: CameraSupportedFormat) =>
    `${f.recordResolution.width}x${f.recordResolution.height}`;

  const formats = $derived(settings?.record.supported.supportedFormats ?? []);
  const groups = $derived([...new Set(formats.map((f) => f.resolutionDescriptor.group))]);
  const format = $derived(formats.find((f) => resolutionKey(f) === draft?.resolution));
  const group = $derived(format?.resolutionDescriptor.group ?? groups[0] ?? '');
  const profiles = $derived(settings?.stream.platform.profiles ?? []);
  const ladder = $derived(profiles.find((p) => p.profile === draft?.quality)?.configs ?? []);
  const card = $derived(
    settings?.storage.workingset.workingset.find(
      (d) => d?.deviceName === settings?.storage.active?.deviceName,
    ) ?? null,
  );
  const recordDirty = $derived(
    !!settings &&
      !!draft &&
      (draft.codec !== settings.record.format.codec ||
        draft.frameRate !== settings.record.format.frameRate ||
        draft.resolution !==
          `${settings.record.format.recordResolution.width}x${settings.record.format.recordResolution.height}`),
  );
  const streamDirty = $derived(
    !!settings &&
      !!draft &&
      (draft.server !== settings.stream.active.server ||
        draft.quality !== settings.stream.active.quality),
  );

  function stage(next: CameraSettings): void {
    settings = next;
    draft = {
      resolution: `${next.record.format.recordResolution.width}x${next.record.format.recordResolution.height}`,
      frameRate: next.record.format.frameRate,
      codec: next.record.format.codec,
      server: next.stream.active.server,
      quality: next.stream.active.quality,
    };
  }

  async function load() {
    loading = true;
    failure = '';
    try {
      stage(await fetchCameraSettings());
    } catch (e) {
      failure = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    sendWsCommand('connectors.state');
    void load();
  });

  /** A format change may strand the frame rate or codec; fall back to what the new one offers. */
  function pickResolution(key: string) {
    const next = formats.find((f) => resolutionKey(f) === key);
    if (!next || !draft) return;
    draft = {
      ...draft,
      resolution: key,
      frameRate: next.frameRates.includes(draft.frameRate)
        ? draft.frameRate
        : (next.frameRates[0] ?? ''),
      codec: next.codecs.includes(draft.codec) ? draft.codec : (next.codecs[0] ?? ''),
    };
  }

  function pickGroup(next: string) {
    const first = formats.find((f) => f.resolutionDescriptor.group === next);
    if (first) pickResolution(resolutionKey(first));
  }

  async function apply() {
    if (!settings || !draft || !format) return;
    const update: CameraSettingsUpdate = {};
    if (recordDirty) {
      update.record = {
        codec: draft.codec,
        frameRate: draft.frameRate,
        recordResolution: format.recordResolution,
        sensorResolution: format.sensorResolution,
      };
    }
    // The camera refuses stream control once the running stream's destination differs
    // from the configured one ("stream is active to a different destination"), and that
    // state can only be cleared at the camera. So the destination is only ever changed
    // while it is idle; a record-only change still goes through.
    if (streamDirty && !streaming) {
      update.stream = { ...settings.stream.active, server: draft.server, quality: draft.quality };
    }
    saving = true;
    try {
      stage(await applyCameraSettings(update));
    } catch (e) {
      pushToast({
        kind: $_('toast.connector'),
        source: 'Blackmagic Camera',
        title: $_('camera.applyFailed'),
        body: String(e),
        tone: 'error',
      });
    } finally {
      saving = false;
    }
  }

  const bytes = (n: number) =>
    new Intl.NumberFormat($locale ?? 'en', {
      style: 'unit',
      unit: n >= 1e12 ? 'terabyte' : 'gigabyte',
      maximumFractionDigits: n >= 1e12 ? 1 : 0,
    }).format(n / (n >= 1e12 ? 1e12 : 1e9));

  const clock = (seconds: number) => {
    const s = Math.max(0, Math.round(seconds));
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${Math.floor(s / 3600)}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
  };

  // The live store is the only source for the transport: the fetched snapshot is a
  // still from load time, and OR-ing it in pinned "streaming" on for the life of the
  // page — the camera stops, the button still offers Stop, and the camera refuses it.
  const streaming = $derived(live.cameraStreaming);
  const recording = $derived(live.cameraRecording);
  const connected = $derived(live.connectorStatus['blackmagic-camera'] === 'connected');

  /** Everything else on the page is a snapshot too; re-read it when the camera moves. */
  async function refresh() {
    try {
      settings = await fetchCameraSettings();
    } catch {
      /* keep the last good snapshot — the transport state is live either way */
    }
  }

  let seen = '';
  $effect(() => {
    const now = `${live.cameraStreamStatus}/${live.cameraRecording}`;
    if (seen && seen !== now) void refresh();
    seen = now;
  });

  // The camera reports the RTMP handshake itself — `Connecting` on the way up,
  // `Flushing` on the way down — so the busy state is read, not guessed at on a
  // timer. Recording is local to the camera and has no transition of its own.
  const streamBusy = $derived(
    live.cameraStreamStatus === 'Connecting' || live.cameraStreamStatus === 'Flushing',
  );

  function transport(kind: 'stream' | 'record', on: boolean) {
    sendWsCommand(`blackmagic-camera.${kind}.${on ? 'start' : 'stop'}`);
  }
</script>

<PageHeader
  title={$_('camera.title')}
  back={{ label: $_('camera.back'), href: '/settings/connectors' }}
>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

{#if loading}
  <div class="centre"><Spinner label={$_('camera.loading')} /></div>
{:else if failure || !settings || !draft}
  <ErrorState
    title={$_('camera.unavailable')}
    body={connected ? failure : $_('camera.notConnected')}
    retryLabel={$_('camera.retry')}
    onretry={load}
  />
{:else}
  <TransportDock
    label={$_('camera.transport')}
    status={recording && streaming
      ? $_('camera.state.both')
      : recording
        ? $_('camera.state.recording')
        : streaming
          ? $_('camera.state.live')
          : $_('camera.state.standby')}
    current={`${settings.stream.active.platform} · ${settings.stream.active.server}`}
    position={card ? clock(card.remainingRecordTime) : '—'}
    actions={[
      {
        icon: recording ? 'stop' : 'record',
        label: recording ? $_('camera.stopRecord') : $_('camera.startRecord'),
        variant: recording ? 'stop' : 'default',
        onclick: () => transport('record', !recording),
      },
      {
        icon: streaming ? 'stop' : 'stream',
        label: streaming ? $_('camera.stopStream') : $_('camera.goLive'),
        variant: streaming ? 'stop' : 'primary',
        disabled: streamBusy || !settings.stream.available.available,
        onclick: () => transport('stream', !streaming),
      },
    ]}
  />

  <section class="overview">
    <OverviewCell
      label={$_('camera.overview.stream')}
      value={streaming ? $_('camera.state.live') : live.cameraStreamStatus}
      color={streaming ? 'var(--status-live)' : 'var(--status-off)'}
      pulse={streaming}
    />
    <OverviewCell
      label={$_('camera.overview.record')}
      value={recording ? $_('camera.state.rec') : $_('camera.state.ready')}
      color={recording ? 'var(--status-live)' : 'var(--status-ok)'}
      divider
    />
    <OverviewCell
      label={$_('camera.overview.free')}
      value={card ? bytes(card.remainingSpace) : '—'}
      color={card ? 'var(--status-ok)' : 'var(--status-warn)'}
      divider
    />
  </section>

  <SectionLabel hint={$_('camera.storage.slots', { values: { n: settings.storage.slots.length } })}>
    {$_('camera.storage.label')}
  </SectionLabel>
  <List>
    {#each settings.storage.slots as slot, i (slot.index)}
      {@const device = settings.storage.workingset.workingset[i] ?? null}
      <Row
        title={device?.volume || $_('camera.storage.empty')}
        meta={device
          ? $_('camera.storage.meta', {
              values: {
                type: slot.type,
                clips: device.clipCount,
                free: bytes(device.remainingSpace),
                total: bytes(device.totalSpace),
              },
            })
          : $_('camera.storage.noCard', { values: { type: slot.type } })}
        detail={device ? clock(device.remainingRecordTime) : ''}
        chevron={false}
        last={i === settings.storage.slots.length - 1}
      >
        {#snippet icon()}<Icon name="card" size={20} stroke={1.5} />{/snippet}
      </Row>
    {/each}
    {#if card}
      <div class="pad">
        <ProgressBar
          label={$_('camera.storage.used', { values: { volume: card.volume } })}
          value={((card.totalSpace - card.remainingSpace) / card.totalSpace) * 100}
          valueText={$_('camera.storage.freeOf', {
            values: { free: bytes(card.remainingSpace), total: bytes(card.totalSpace) },
          })}
        />
      </div>
    {/if}
  </List>

  <SectionLabel
    hint={format ? `${format.recordResolution.width} × ${format.recordResolution.height}` : ''}
  >
    {$_('camera.record.label')}
  </SectionLabel>
  <List>
    <div class="pad">
      <Segmented
        label={$_('camera.record.sensor')}
        value={group}
        options={groups.map((g) => ({ value: g, label: g }))}
        onchange={pickGroup}
      />
      <RadioGroup
        label={$_('camera.record.resolution')}
        value={draft.resolution}
        options={formats
          .filter((f) => f.resolutionDescriptor.group === group)
          .map((f) => ({
            value: resolutionKey(f),
            label: f.resolutionDescriptor.description,
            hint: `${f.resolutionDescriptor.aspectRatio} · ${f.recordResolution.width} × ${f.recordResolution.height}`,
          }))}
        onchange={pickResolution}
      />
      <Select
        label={$_('camera.record.frameRate')}
        value={draft.frameRate}
        options={(format?.frameRates ?? []).map((r) => ({
          value: r,
          label: $_('camera.record.fps', { values: { r } }),
        }))}
        onchange={(r) => (draft = draft ? { ...draft, frameRate: r } : draft)}
      />
      <Select
        label={$_('camera.record.codec')}
        value={draft.codec}
        options={(format?.codecs ?? []).map((c) => ({
          value: c,
          label: c.replace('BRaw:', 'BRaw ').replace('_', ':'),
        }))}
        onchange={(c) => (draft = draft ? { ...draft, codec: c } : draft)}
      />
      {#if card}
        <Stat label={$_('camera.record.remaining')} value={clock(card.remainingRecordTime)} />
      {/if}
    </div>
  </List>

  <SectionLabel hint={settings.stream.active.platform}>{$_('camera.stream.label')}</SectionLabel>
  <List>
    <div class="pad">
      <Segmented
        label={$_('camera.stream.server')}
        value={draft.server}
        options={settings.stream.platform.servers.map((s) => ({
          value: s.server,
          label: s.server,
        }))}
        onchange={(s) => (draft = draft ? { ...draft, server: s } : draft)}
      />
      <Select
        label={$_('camera.stream.quality')}
        value={draft.quality}
        options={profiles.map((p) => ({
          value: p.profile,
          label: p.configs.length
            ? p.profile
            : $_('camera.stream.noTable', { values: { profile: p.profile } }),
        }))}
        onchange={(q) => (draft = draft ? { ...draft, quality: q } : draft)}
      />
    </div>
    {#if ladder.length}
      {#each ladder as config (`${config.resolution}-${config.fps}`)}
        <Row
          title={$_('camera.stream.rung', {
            values: { resolution: config.resolution, fps: config.fps },
          })}
          meta={config.videoCodecs.join(' · ')}
          detail={$_('camera.stream.mbps', { values: { rate: (config.bitrate / 1e6).toFixed(1) } })}
          chevron={false}
        />
      {/each}
    {:else}
      <div class="pad"><p class="note">{$_('camera.stream.noTableNote')}</p></div>
    {/if}
    <div class="pad">
      <Row
        title={$_('camera.stream.output')}
        meta={settings.stream.status.effectiveVideoFormat}
        detail={settings.stream.available.available
          ? $_('camera.stream.ready')
          : settings.stream.available.reasons.join(', ')}
        chevron={false}
        last
      />
    </div>
  </List>

  {#if recordDirty || streamDirty}
    <StickyActionBar
      primary={saving ? $_('camera.applying') : $_('camera.apply')}
      secondary={$_('camera.revert')}
      primaryDisabled={saving || (streamDirty && !recordDirty && streaming)}
      onprimary={apply}
      onsecondary={() => settings && stage(settings)}
    />
  {/if}

  <p class="note footer">{$_('camera.note')}</p>
{/if}

<style>
  .overview {
    margin: 0 24px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border-block: 1px solid var(--border-hairline);
  }
  .centre {
    display: flex;
    justify-content: center;
    padding: 64px 24px;
  }
  .pad {
    padding: 14px 24px 16px;
    display: grid;
    /* Holds the controls to the column they were given; an auto track would take
       a wide segmented's max-content width and push the page sideways. */
    grid-template-columns: minmax(0, 1fr);
    gap: 14px;
  }
  .note {
    margin: 0;
    font-family: var(--font-display);
    font-style: italic;
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1.45;
  }
  .footer {
    padding: 28px 32px 96px;
    text-align: center;
  }
</style>
