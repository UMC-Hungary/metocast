<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/state';
  import { _ } from 'svelte-i18n';
  import { siObsstudio, siYoutube, siFacebook, siBlackmagicdesign, siDiscord } from 'simple-icons';
  import {
    PageHeader,
    SectionLabel,
    List,
    Row,
    Glyph,
    Dot,
    Toggle,
    Field,
    Button,
    Segmented,
    Stat,
    OverviewCell,
    DiscoveryPanel,
    ToggleRow,
    Select,
  } from '@metocast/design-system';
  import {
    discoverRodecasterAudio,
    fetchConnectorStatuses,
    fetchConnectorConfig,
    hostCapabilities,
    openExternal,
    pickDirectory,
    saveConnectorConfig,
    connectObs,
    disconnectObs,
    fetchObsStreamSettings,
    applyObsStreamSettings,
    fetchYouTubeStreamKey,
    fetchFacebookStreamKey,
    fetchDevices,
    triggerDiscover,
    discoverCameras,
    discoverMiddlecontrol,
    pushCameraYouTubeSettings,
    pushAtemYouTubeSettings,
    discoverAtem,
    type ConnectorName,
    type ConnectorConfigMap,
    type BroadlinkDevice,
    type DiscoveredCamera,
    type DiscoveredMiddlecontrol,
    type DiscoveredAtem,
    type RodecasterAudioRecordingConfig,
  } from '@metocast/core-client';
  import { appMode } from '$lib/core';
  import { rode } from '$lib/rode';
  import { live } from '$lib/live.svelte';
  import LoginSheet from '$lib/LoginSheet.svelte';
  import { pushToast, resolveByKey } from '$lib/notifications.svelte';
  import NotifBell from '$lib/NotifBell.svelte';

  type ConnState = 'disconnected' | 'connecting' | 'connected' | 'error';
  type Category = 'broadcast' | 'streaming' | 'devices' | 'future';
  type InputSpec = { key: string; type: 'text' | 'number' | 'password'; secret: boolean };
  type ToggleSpec = { key: string; type: 'toggle' };
  type FieldSpec = InputSpec | ToggleSpec;

  const MIDDLECONTROL_SDK_URL =
    'https://www.middlethings.co/support/docs/joysticks-and-controllers/external-sdk-api/';

  interface ConnMeta {
    id: ConnectorName;
    name: string;
    cat: Category;
    supported: boolean;
    char?: string;
    brand?: string;
    /** Only for a mark that is not drawn in `simple-icons`' 24x24 box. */
    brandBox?: string;
    fields: FieldSpec[];
  }

  const fld = (key: string, type: InputSpec['type'] = 'text', secret = false): InputSpec => ({
    key,
    type,
    secret,
  });

  const tgl = (key: string): ToggleSpec => ({ key, type: 'toggle' });

  const isToggle = (f: FieldSpec): f is ToggleSpec => f.type === 'toggle';
  const isInput = (f: FieldSpec): f is InputSpec => f.type !== 'toggle';

  const CONNECTORS: ConnMeta[] = [
    {
      id: 'obs',
      name: 'OBS Studio',
      cat: 'broadcast',
      supported: true,
      brand: siObsstudio.path,
      fields: [fld('host'), fld('port', 'number'), fld('password', 'password', true)],
    },
    {
      id: 'youtube',
      name: 'YouTube',
      cat: 'streaming',
      supported: true,
      brand: siYoutube.path,
      fields: [fld('clientId'), fld('clientSecret', 'password', true)],
    },
    {
      id: 'facebook',
      name: 'Facebook',
      cat: 'streaming',
      supported: true,
      brand: siFacebook.path,
      fields: [fld('appId'), fld('appSecret', 'password', true), fld('pageId')],
    },
    {
      id: 'broadlink',
      name: 'Broadlink RF/IR',
      cat: 'devices',
      supported: true,
      char: '⌁',
      fields: [],
    },
    {
      id: 'blackmagic-camera',
      name: 'Blackmagic Camera',
      cat: 'devices',
      supported: true,
      brand: siBlackmagicdesign.path,
      fields: [fld('host'), fld('username'), fld('password', 'password', true), fld('fingerprint')],
    },
    {
      id: 'rodecaster',
      name: 'RØDECaster Pro II',
      cat: 'devices',
      supported: true,
      brand: rode.path,
      brandBox: rode.viewBox,
      fields: [tgl('notifyOnMute')],
    },
    {
      id: 'middlecontrol',
      name: 'Middle Control',
      cat: 'devices',
      supported: true,
      char: 'MC',
      fields: [fld('host'), fld('port', 'number')],
    },
    {
      id: 'atem',
      name: 'Blackmagic ATEM',
      cat: 'devices',
      supported: true,
      brand: siBlackmagicdesign.path,
      fields: [fld('host'), fld('port', 'number')],
    },
    {
      id: 'vmix',
      name: 'vMix',
      cat: 'future',
      supported: false,
      char: '▣',
      fields: [fld('host'), fld('port', 'number')],
    },
    {
      id: 'discord',
      name: 'Discord',
      cat: 'future',
      supported: false,
      brand: siDiscord.path,
      fields: [fld('webhookUrl', 'password', true)],
    },
  ];

  const CATS: Category[] = ['broadcast', 'streaming', 'devices', 'future'];

  const builders: {
    [K in ConnectorName]: (form: Record<string, string>, enabled: boolean) => ConnectorConfigMap[K];
  } = {
    obs: (form, enabled) => ({
      enabled,
      host: form.host ?? '',
      port: Number(form.port) || 0,
      password: form.password || null,
    }),
    vmix: (form, enabled) => ({ enabled, host: form.host ?? '', port: Number(form.port) || 0 }),
    atem: (form, enabled) => ({ enabled, host: form.host ?? '', port: Number(form.port) || 0 }),
    middlecontrol: (form, enabled) => ({
      enabled,
      host: form.host ?? '',
      port: Number(form.port) || 0,
    }),
    broadlink: (_form, enabled) => ({ enabled }),
    rodecaster: (_form, enabled) => ({
      enabled,
      notifyOnMute: toggles.rodecaster?.notifyOnMute ?? false,
      audioRecording: rodecasterAudioRecording,
    }),
    'blackmagic-camera': (form, enabled) => ({
      enabled,
      host: form.host ?? '',
      fingerprint: form.fingerprint ?? '',
      username: form.username ?? '',
      password: form.password ?? '',
    }),
    youtube: (form, enabled) => ({
      enabled,
      clientId: form.clientId ?? '',
      clientSecret: form.clientSecret ?? '',
    }),
    facebook: (form, enabled) => ({
      enabled,
      appId: form.appId ?? '',
      appSecret: form.appSecret ?? '',
      pageId: form.pageId ?? '',
    }),
    discord: (form, enabled) => ({ enabled, webhookUrl: form.webhookUrl ?? '' }),
    szentiras: (form, enabled) => ({ enabled, apiKey: form.apiKey ?? '' }),
  };

  let statuses = $state<Partial<Record<ConnectorName, ConnState>>>({});
  let enabled = $state<Record<string, boolean>>({});
  let forms = $state<Record<string, Record<string, string>>>(
    Object.fromEntries(
      CONNECTORS.map((c) => [c.id, Object.fromEntries(c.fields.map((f) => [f.key, '']))]),
    ),
  );
  let toggles = $state<Record<string, Record<string, boolean>>>(
    Object.fromEntries(
      CONNECTORS.map((c) => [
        c.id,
        Object.fromEntries(c.fields.filter(isToggle).map((f) => [f.key, false])),
      ]),
    ),
  );
  let rodecasterAudioRecording = $state<RodecasterAudioRecordingConfig>({
    schemaVersion: 1,
    enabled: false,
    directory: '',
    processingMode: 'preFader',
    outputMode: 'mainMix',
    sources: [],
  });
  let secretKept = $state<Record<string, string[]>>({});
  let expanded = $state<ConnectorName | null>('obs');
  let loginProvider = $state<'youtube' | 'facebook' | null>(null);

  let destination = $state<'youtube' | 'facebook'>('youtube');
  let rtmp = $state('');
  let devices = $state<BroadlinkDevice[]>([]);
  let cameras = $state<DiscoveredCamera[]>([]);
  let cameraRtmp = $state('');
  let pushingCamera = $state(false);
  let atemDestination = $state('');
  let pushingAtem = $state(false);
  let atemDevices = $state<DiscoveredAtem[]>([]);
  let scanningAtem = $state(false);
  let atemScanned = $state(false);
  let scanning = $state(false);
  let scanningCameras = $state(false);
  let middlecontrolDevices = $state<DiscoveredMiddlecontrol[]>([]);
  let scanningMiddlecontrol = $state(false);
  let middlecontrolScanned = $state(false);

  function fail(meta: ConnMeta, titleKey: string, message: string) {
    pushToast({
      kind: $_('toast.connector'),
      source: meta.name,
      title: $_(titleKey),
      body: message,
      tone: 'error',
    });
  }

  const byCat = (cat: Category) => CONNECTORS.filter((c) => c.cat === cat);
  const enabledCount = $derived(CONNECTORS.filter((c) => enabled[c.id]).length);
  const readyCount = $derived(
    CONNECTORS.filter((c) => c.supported && enabled[c.id] && statuses[c.id] !== 'connected').length,
  );
  const liveCount = $derived(CONNECTORS.filter((c) => statuses[c.id] === 'connected').length);
  const futureCount = $derived(CONNECTORS.filter((c) => !c.supported).length);
  const obsConnected = $derived(statuses.obs === 'connected');
  const ytLinked = $derived(statuses.youtube === 'connected');
  const rodecasterAudioEndpoint = $derived(live.rodecasterAudioDiscovery?.endpoint ?? null);
  const canPickRecordingDirectory = $derived(hostCapabilities.dialogs && appMode() === 'server');
  const rodecasterAudioSources = $derived(
    rodecasterAudioEndpoint?.sources.filter((source) => source.identity.kind === 'faderSlot') ?? [],
  );
  const middlecontrolCameraIds = $derived(
    live.middlecontrolState?.connectedCameraIds ?? [],
  );
  const middlecontrolApcrIds = $derived(live.middlecontrolState?.connectedApcrIds ?? []);
  const middlecontrolRecordingIds = $derived(
    live.middlecontrolState?.recordingCameraIds ?? [],
  );
  const middlecontrolDeviceIds = $derived(
    [...new Set([...middlecontrolCameraIds, ...middlecontrolApcrIds])].sort((a, b) => a - b),
  );
  const middlecontrolConnected = $derived(
    (live.connectorStatus.middlecontrol ?? statuses.middlecontrol) === 'connected',
  );
  const recordingSourceSelectionMissing = $derived(
    rodecasterAudioRecording.enabled &&
      rodecasterAudioRecording.outputMode !== 'mainMix' &&
      rodecasterAudioRecording.sources.length === 0,
  );

  function dotColor(meta: ConnMeta): string {
    if (!enabled[meta.id] || !meta.supported) return 'var(--status-off)';
    switch (statuses[meta.id]) {
      case 'connected':
        return 'var(--status-ok)';
      case 'connecting':
        return 'var(--status-warn)';
      case 'error':
        return 'var(--status-error)';
      default:
        return 'var(--status-off)';
    }
  }

  function detailText(meta: ConnMeta): string {
    if (!enabled[meta.id]) return $_('conn.disabled');
    if (
      meta.id === 'obs' ||
      meta.id === 'blackmagic-camera' ||
      meta.id === 'middlecontrol' ||
      meta.id === 'atem'
    ) {
      const f = forms[meta.id] ?? {};
      const address =
        meta.id === 'blackmagic-camera'
          ? f.host || '—'
          : `${f.host || (meta.id === 'obs' ? 'localhost' : '—')}:${
              f.port || { obs: '4455', middlecontrol: '11584', atem: '9910' }[meta.id]
            }`;
      return `${address} · ${statuses[meta.id] ?? 'disconnected'}`;
    }
    return $_(`conn.descriptor.${meta.id}`);
  }

  async function loadConfig(meta: ConnMeta) {
    const cfg = await fetchConnectorConfig(meta.id);
    if ('audioRecording' in cfg) rodecasterAudioRecording = cfg.audioRecording;
    const form: Record<string, string> = {};
    const flags: Record<string, boolean> = {};
    const kept: string[] = [];
    for (const [k, v] of Object.entries(cfg)) {
      if (k === 'enabled') enabled[meta.id] = v === true;
      else if (k.endsWith('Set')) {
        if (v === true) kept.push(k.slice(0, -3));
      } else if (typeof v === 'boolean') flags[k] = v;
      else if (typeof v === 'string' || typeof v === 'number') form[k] = String(v);
    }
    forms[meta.id] = form;
    toggles[meta.id] = flags;
    secretKept[meta.id] = kept;
  }

  async function refreshStatuses() {
    try {
      const s = await fetchConnectorStatuses();
      const next: Partial<Record<ConnectorName, ConnState>> = {};
      for (const c of CONNECTORS) next[c.id] = s[c.id]?.type;
      statuses = next;
    } catch {
      /* offline */
    }
  }

  let poll: ReturnType<typeof setInterval> | undefined;
  onMount(async () => {
    await refreshStatuses();
    await Promise.all(CONNECTORS.map((c) => loadConfig(c).catch(() => {})));
    poll = setInterval(refreshStatuses, 4000);
    const requested = page.url.searchParams.get('open') as ConnectorName | null;
    const meta = requested && CONNECTORS.find((c) => c.id === requested);
    if (meta) await open(meta);
  });
  onDestroy(() => clearInterval(poll));

  async function toggle(meta: ConnMeta) {
    if (!meta.supported) return;
    const next = !enabled[meta.id];
    enabled[meta.id] = next;
    try {
      await saveConnectorConfig(meta.id, builders[meta.id](forms[meta.id] ?? {}, next));
      // A connector the operator switched off is no longer an open issue: its status
      // falls to `disconnected`, which never resolves the failure on its own.
      if (!next) resolveByKey(`connector:${meta.id}`);
      await refreshStatuses();
    } catch (e) {
      enabled[meta.id] = !next;
      fail(meta, 'conn.toast.toggleFail', String(e));
    }
  }

  async function open(meta: ConnMeta) {
    expanded = expanded === meta.id ? null : meta.id;
    if (expanded !== meta.id) return;
    if (meta.id === 'broadlink') devices = await fetchDevices().catch(() => []);
    if (meta.id === 'rodecaster') discoverRodecasterAudio();
  }

  function recordingSlotSelected(number: number): boolean {
    return rodecasterAudioRecording.sources.some(
      (source) => source.kind === 'faderSlot' && source.number === number,
    );
  }

  function toggleRecordingSlot(number: number): void {
    const sources = recordingSlotSelected(number)
      ? rodecasterAudioRecording.sources.filter(
          (source) => source.kind !== 'faderSlot' || source.number !== number,
        )
      : [...rodecasterAudioRecording.sources, { kind: 'faderSlot' as const, number }];
    rodecasterAudioRecording = { ...rodecasterAudioRecording, sources };
  }

  async function chooseRecordingDirectory(): Promise<void> {
    const directory = await pickDirectory($_('rodecaster.recording.chooseDirectory'));
    if (directory) rodecasterAudioRecording = { ...rodecasterAudioRecording, directory };
  }

  async function setFlag(meta: ConnMeta, key: string, checked: boolean) {
    const previous = toggles[meta.id]?.[key] ?? false;
    toggles[meta.id] = { ...(toggles[meta.id] ?? {}), [key]: checked };
    try {
      await saveConnectorConfig(
        meta.id,
        builders[meta.id](forms[meta.id] ?? {}, enabled[meta.id] ?? false),
      );
    } catch (e) {
      toggles[meta.id] = { ...(toggles[meta.id] ?? {}), [key]: previous };
      fail(meta, 'conn.toast.saveFail', String(e));
    }
  }

  async function save(meta: ConnMeta) {
    try {
      await saveConnectorConfig(
        meta.id,
        builders[meta.id](forms[meta.id] ?? {}, enabled[meta.id] ?? false),
      );
      pushToast({
        kind: $_('toast.connector'),
        source: meta.name,
        title: meta.id === 'rodecaster' ? $_('rodecaster.recording.saved') : $_('conn.saved'),
        tone: 'ok',
      });
      await loadConfig(meta);
      await refreshStatuses();
    } catch (e) {
      fail(meta, 'conn.toast.saveFail', String(e));
    }
  }

  const obsMeta = CONNECTORS.find((c) => c.id === 'obs')!;
  async function toggleObs() {
    try {
      if (obsConnected) await disconnectObs();
      else await connectObs();
    } catch (e) {
      fail(
        obsMeta,
        obsConnected ? 'conn.toast.disconnectFail' : 'conn.toast.connectFail',
        String(e),
      );
    }
    await refreshStatuses();
    if (statuses.obs !== 'connected') return;
    const s = await fetchObsStreamSettings().catch(() => null);
    if (s?.server) rtmp = s.server;
  }

  async function pickDestination(dest: 'youtube' | 'facebook') {
    destination = dest;
    if (!obsConnected) return;
    try {
      const key =
        dest === 'youtube' ? await fetchYouTubeStreamKey() : await fetchFacebookStreamKey();
      rtmp = key.rtmpUrl;
      const slash = rtmp.lastIndexOf('/');
      await applyObsStreamSettings(
        slash > 6 ? rtmp.slice(0, slash) : rtmp,
        slash > 6 ? rtmp.slice(slash + 1) : '',
      );
    } catch (e) {
      fail(obsMeta, 'conn.toast.saveFail', String(e));
    }
  }

  const cameraMeta = CONNECTORS.find((c) => c.id === 'blackmagic-camera')!;
  // A scan is also the connect action: the core adopts the first camera it finds
  // when none is configured, so reload the config and status afterwards.
  async function scanCameras() {
    scanningCameras = true;
    try {
      cameras = await discoverCameras();
      await loadConfig(cameraMeta);
      await refreshStatuses();
    } catch (e) {
      fail(cameraMeta, 'conn.toast.scanFail', String(e));
    } finally {
      scanningCameras = false;
    }
  }

  const middlecontrolMeta = CONNECTORS.find((c) => c.id === 'middlecontrol')!;
  function useMiddlecontrol(host: string, port?: number) {
    const current = forms.middlecontrol ?? {};
    forms.middlecontrol = {
      ...current,
      host,
      port: port === undefined ? current.port || '11584' : String(port),
    };
  }

  async function scanMiddlecontrol() {
    scanningMiddlecontrol = true;
    try {
      middlecontrolDevices = await discoverMiddlecontrol();
      middlecontrolScanned = true;
    } catch (e) {
      fail(middlecontrolMeta, 'conn.toast.scanFail', String(e));
    } finally {
      scanningMiddlecontrol = false;
    }
  }

  // Sets the camera's livestream destination; it does not go live.
  async function pushCameraYoutube() {
    pushingCamera = true;
    try {
      cameraRtmp = (await pushCameraYouTubeSettings()).rtmpUrl;
    } catch (e) {
      fail(cameraMeta, 'conn.toast.saveFail', String(e));
    } finally {
      pushingCamera = false;
    }
  }

  const atemMeta = CONNECTORS.find((c) => c.id === 'atem')!;
  async function scanAtem() {
    scanningAtem = true;
    try {
      atemDevices = await discoverAtem();
      atemScanned = true;
    } catch (e) {
      fail(atemMeta, 'conn.toast.scanFail', String(e));
    } finally {
      scanningAtem = false;
    }
  }

  async function useAtem(device: DiscoveredAtem) {
    forms.atem = { ...(forms.atem ?? {}), host: device.host, port: String(device.port) };
    enabled.atem = true;
    await save(atemMeta);
  }

  async function pushAtemYoutube() {
    pushingAtem = true;
    try {
      const target = await pushAtemYouTubeSettings();
      atemDestination = `${target.service} · ${target.url}`;
    } catch (e) {
      fail(atemMeta, 'conn.toast.saveFail', String(e));
    } finally {
      pushingAtem = false;
    }
  }

  const broadlinkMeta = CONNECTORS.find((c) => c.id === 'broadlink')!;
  async function scan() {
    scanning = true;
    try {
      await triggerDiscover();
      devices = await fetchDevices();
    } catch (e) {
      fail(broadlinkMeta, 'conn.toast.scanFail', String(e));
    } finally {
      scanning = false;
    }
  }
</script>

<PageHeader
  title={$_('screens.connectors.title')}
  back={{ label: $_('conn.back'), href: '/settings' }}
>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

<div class="connectors-workspace">
  <div class="main-col">
    <section class="overview">
      <OverviewCell label={$_('conn.summary.live')} value={liveCount} color="var(--status-live)" />
      <OverviewCell
        label={$_('conn.summary.ready')}
        value={readyCount}
        color="var(--status-ok)"
        divider
      />
      <OverviewCell
        label={$_('conn.summary.future')}
        value={futureCount}
        color="var(--status-warn)"
        divider
      />
    </section>

    {#each CATS as cat (cat)}
      {@const items = byCat(cat)}
      {#if items.length}
        <SectionLabel
          hint={$_('conn.enabledHint', {
            values: { n: items.filter((c) => enabled[c.id]).length, total: items.length },
          })}
        >
          {$_(`conn.cat.${cat}`)}
        </SectionLabel>
        <List>
          {#each items as meta (meta.id)}
            {@const form = forms[meta.id] ?? {}}
            <div class="connector" class:muted={!enabled[meta.id]}>
              <button class="head" type="button" onclick={() => open(meta)}>
                {#if meta.brand}
                  <Glyph size={34}>
                    {#snippet mark()}<svg
                        viewBox={meta.brandBox ?? '0 0 24 24'}
                        width="19"
                        height="19"
                        fill="currentColor"
                        aria-hidden="true"><path d={meta.brand} /></svg
                      >{/snippet}
                  </Glyph>
                {:else}
                  <Glyph char={meta.char} size={34} />
                {/if}
                <span class="text">
                  <strong>{meta.name}</strong>
                  <em><Dot color={dotColor(meta)} size={5} />{detailText(meta)}</em>
                </span>
              </button>
              <Toggle
                checked={enabled[meta.id] ?? false}
                disabled={!meta.supported}
                label={meta.name}
                onchange={() => toggle(meta)}
              />
            </div>

            {#if expanded === meta.id}
              <section class="detail">
                {#if !meta.supported}<p>{$_('conn.lockedNote')}</p>{/if}

                {#each meta.fields.filter(isToggle) as field (field.key)}
                  <ToggleRow
                    label={$_(`conn.field.${field.key}`)}
                    sub={$_(`conn.fieldHint.${field.key}`)}
                    checked={toggles[meta.id]?.[field.key] ?? false}
                    disabled={!meta.supported}
                    onchange={(checked) => setFlag(meta, field.key, checked)}
                  />
                {/each}

                {#each meta.fields.filter(isInput) as field (field.key)}
                  <Field
                    label={$_(`conn.field.${field.key}`)}
                    type={field.type}
                    bind:value={form[field.key]}
                    readonly={!meta.supported}
                    placeholder={field.secret && secretKept[meta.id]?.includes(field.key)
                      ? '••••••••'
                      : meta.id === 'middlecontrol' && field.key === 'host'
                        ? 'localhost'
                        : ''}
                    hint={field.secret && secretKept[meta.id]?.includes(field.key)
                      ? $_('conn.secretKept')
                      : ''}
                  />
                {/each}

                {#if meta.id === 'middlecontrol'}
                  <p class="note">{$_('conn.middlecontrol.help')}</p>
                  <div class="actions">
                    <Button
                      variant="secondary"
                      compact
                      onclick={() => useMiddlecontrol('localhost')}
                      >{$_('conn.middlecontrol.useLocal')}</Button
                    >
                    <Button variant="secondary" compact onclick={() => save(meta)}
                      >{$_('conn.encoder.reconnect')}</Button
                    >
                    <Button
                      variant="quiet"
                      compact
                      onclick={() => void openExternal(MIDDLECONTROL_SDK_URL)}
                    >{$_('conn.middlecontrol.sdk')} ↗</Button>
                  </div>
                  <section
                    class="middlecontrol-devices"
                    aria-labelledby="middlecontrol-devices-title"
                  >
                    <div class="middlecontrol-devices-head">
                      <h3 id="middlecontrol-devices-title">
                        {$_('conn.middlecontrol.devicesTitle')}
                      </h3>
                      <span>
                        {$_('conn.middlecontrol.devicesSummary', {
                          values: {
                            cameras: middlecontrolCameraIds.length,
                            ptz: middlecontrolApcrIds.length,
                          },
                        })}
                      </span>
                    </div>
                    {#if !middlecontrolConnected}
                      <p class="empty">{$_('conn.middlecontrol.devicesDisconnected')}</p>
                    {:else if !live.middlecontrolState}
                      <p class="empty">{$_('conn.middlecontrol.devicesWaiting')}</p>
                    {:else if middlecontrolDeviceIds.length === 0}
                      <p class="empty">{$_('conn.middlecontrol.devicesEmpty')}</p>
                    {:else}
                      <ul>
                        {#each middlecontrolDeviceIds as id (id)}
                          <li>
                            <span class="middlecontrol-device-name">
                              <strong>
                                {$_('conn.middlecontrol.camera', { values: { id } })}
                              </strong>
                              <em>
                                {middlecontrolCameraIds.includes(id)
                                  ? $_('conn.middlecontrol.connected')
                                  : $_('conn.middlecontrol.cameraOffline')}
                              </em>
                            </span>
                            <span class="middlecontrol-badges">
                              {#if live.middlecontrolState.selectedCamera === id}
                                <span>{$_('conn.middlecontrol.selected')}</span>
                              {/if}
                              {#if middlecontrolRecordingIds.includes(id)}
                                <span class="recording">{$_('conn.middlecontrol.recording')}</span>
                              {/if}
                              {#if middlecontrolApcrIds.includes(id)}
                                <span class="ptz">{$_('conn.middlecontrol.ptz')}</span>
                              {/if}
                            </span>
                          </li>
                        {/each}
                      </ul>
                    {/if}
                  </section>
                  <DiscoveryPanel
                    title={$_('conn.middlecontrol.discoveryTitle')}
                    description={$_('conn.middlecontrol.discoveryDescription')}
                    scanLabel={middlecontrolScanned
                      ? $_('conn.discovery.scanAgain')
                      : $_('conn.discovery.scan')}
                    scanning={scanningMiddlecontrol}
                    scanningLabel={$_('conn.discovery.scanning')}
                    onscan={scanMiddlecontrol}
                  >
                    {#if middlecontrolDevices.length === 0}
                      <p class="empty">
                        {$_(
                          middlecontrolScanned
                            ? 'conn.middlecontrol.discoveryEmpty'
                            : 'conn.middlecontrol.discoveryReady',
                        )}
                      </p>
                    {:else}
                      <ul class="devices">
                        {#each middlecontrolDevices as device (`${device.host}:${device.port}`)}
                          <li>
                            <strong>Middle Control</strong>
                            <Button
                              variant="secondary"
                              compact
                              onclick={() => useMiddlecontrol(device.host, device.port)}
                            >
                              {$_('conn.middlecontrol.useEndpoint', {
                                values: { address: `${device.host}:${device.port}` },
                              })}
                            </Button>
                          </li>
                        {/each}
                      </ul>
                    {/if}
                  </DiscoveryPanel>
                {/if}

                {#if meta.id === 'youtube' || meta.id === 'facebook'}
                  <div class="actions">
                    <Button variant="secondary" compact onclick={() => save(meta)}
                      >{$_('conn.save')}</Button
                    >
                    <Button
                      variant="secondary"
                      compact
                      onclick={() =>
                        (loginProvider = meta.id === 'youtube' ? 'youtube' : 'facebook')}
                    >
                      {statuses[meta.id] === 'connected' ? $_('conn.account') : $_('conn.login')}
                    </Button>
                  </div>
                {/if}

                {#if meta.id === 'obs'}
                  <div class="actions">
                    <Button variant="secondary" compact onclick={toggleObs}>
                      {obsConnected ? $_('conn.encoder.disconnect') : $_('conn.encoder.connect')}
                    </Button>
                    <Button variant="secondary" compact onclick={() => save(meta)}
                      >{$_('conn.encoder.reconnect')}</Button
                    >
                  </div>
                  <Segmented
                    compact
                    label={$_('conn.encoder.rtmp', { values: { dest: destination } })}
                    value={destination}
                    options={[
                      { value: 'youtube', label: 'YouTube' },
                      { value: 'facebook', label: 'Facebook' },
                    ]}
                    onchange={(v) => pickDestination(v as 'youtube' | 'facebook')}
                  />
                  <Field
                    label={$_('conn.encoder.rtmp', { values: { dest: destination } })}
                    value={rtmp}
                    readonly
                  />
                {/if}

                {#if meta.id === 'blackmagic-camera'}
                  <div class="actions">
                    <Button
                      variant="secondary"
                      compact
                      disabled={statuses['blackmagic-camera'] !== 'connected' ||
                        pushingCamera ||
                        live.cameraStreaming}
                      onclick={pushCameraYoutube}
                    >
                      {$_('conn.camera.pushYoutube')}
                    </Button>
                    <Button variant="secondary" compact onclick={() => save(meta)}
                      >{$_('conn.encoder.reconnect')}</Button
                    >
                  </div>
                  <Field label={$_('conn.camera.rtmp')} value={cameraRtmp} readonly />

                  <List>
                    <Row
                      title={$_('conn.camera.control')}
                      meta={$_('conn.camera.controlMeta')}
                      detail={$_('conn.camera.open')}
                      href="/settings/connectors/camera"
                      last
                    >
                      {#snippet icon()}<Glyph char="▶" size={28} />{/snippet}
                    </Row>
                  </List>

                  <DiscoveryPanel
                    title={$_('conn.cameraDiscovery.title')}
                    description={$_('conn.cameraDiscovery.description')}
                    scanLabel={cameras.length
                      ? $_('conn.discovery.scanAgain')
                      : $_('conn.discovery.scan')}
                    scanning={scanningCameras}
                    scanningLabel={$_('conn.discovery.scanning')}
                    onscan={scanCameras}
                  >
                    {#if cameras.length === 0}
                      <p class="empty">{$_('conn.cameraDiscovery.empty')}</p>
                    {:else}
                      <ul class="devices">
                        {#each cameras as c (c.uniqueId)}
                          <li>
                            <strong>{c.deviceName || c.productName}</strong><code>{c.host}</code
                            >{#if c.softwareVersion}<em
                                >{$_('conn.cameraDiscovery.firmware', {
                                  values: { version: c.softwareVersion },
                                })}</em
                              >{/if}
                          </li>
                        {/each}
                      </ul>
                    {/if}
                  </DiscoveryPanel>
                {/if}

                {#if meta.id === 'atem'}
                  <div class="actions">
                    <Button
                      variant="secondary"
                      compact
                      disabled={statuses.atem !== 'connected' ||
                        pushingAtem ||
                        live.atemState?.streaming !== 'idle'}
                      onclick={pushAtemYoutube}
                    >
                      {$_('conn.camera.pushYoutube')}
                    </Button>
                  </div>
                  <Field
                    label={$_('conn.atem.destination')}
                    value={atemDestination || (live.atemState?.streamService ?? '')}
                    readonly
                  />
                  <List>
                    <Row
                      title={$_('conn.atem.control')}
                      meta={$_('conn.atem.controlMeta')}
                      detail={$_('conn.camera.open')}
                      href="/settings/connectors/switcher"
                      last
                    >
                      {#snippet icon()}<Glyph char="▶" size={28} />{/snippet}
                    </Row>
                  </List>

                  <DiscoveryPanel
                    title={$_('conn.atem.discoveryTitle')}
                    description={$_('conn.atem.discoveryDescription')}
                    scanLabel={atemScanned
                      ? $_('conn.discovery.scanAgain')
                      : $_('conn.discovery.scan')}
                    scanning={scanningAtem}
                    scanningLabel={$_('conn.discovery.scanning')}
                    onscan={scanAtem}
                  >
                    {#if atemDevices.length === 0}
                      <p class="empty">
                        {$_(atemScanned ? 'conn.atem.discoveryEmpty' : 'conn.atem.discoveryReady')}
                      </p>
                    {:else}
                      <ul class="devices">
                        {#each atemDevices as device (device.host)}
                          <li>
                            <strong>{device.name}</strong>
                            <Button variant="secondary" compact onclick={() => useAtem(device)}>
                              {$_('conn.atem.use')}
                            </Button>
                            <code
                              >{device.host} · {$_(
                                device.usb ? 'conn.atem.usb' : 'conn.atem.network',
                              )}</code
                            >
                          </li>
                        {/each}
                      </ul>
                    {/if}
                  </DiscoveryPanel>
                {/if}

                {#if meta.id === 'rodecaster'}
                  <p class="note">{$_('conn.rodecaster.exclusive')}</p>
                  <div class="recording-config">
                    <h3>{$_('rodecaster.recording.title')}</h3>
                    <p class="recording-help">{$_('rodecaster.recording.description')}</p>
                    <ToggleRow
                      label={$_('rodecaster.recording.enabled')}
                      sub={$_('rodecaster.recording.enabledHint')}
                      checked={rodecasterAudioRecording.enabled}
                      onchange={(recordingEnabled) =>
                        (rodecasterAudioRecording = {
                          ...rodecasterAudioRecording,
                          enabled: recordingEnabled,
                        })}
                    />
                    {#if rodecasterAudioRecording.enabled && !rodecasterAudioEndpoint}
                      <div class="recording-warning">
                        <p>{$_('rodecaster.recording.endpointUnavailable')}</p>
                        <Button variant="secondary" compact onclick={discoverRodecasterAudio}>
                          {$_('rodecaster.recording.scanAudio')}
                        </Button>
                      </div>
                    {/if}
                    <Field
                      label={$_('rodecaster.recording.directory')}
                      bind:value={rodecasterAudioRecording.directory}
                      placeholder={$_('rodecaster.recording.directoryPlaceholder')}
                      hint={canPickRecordingDirectory
                        ? $_('rodecaster.recording.directoryHint')
                        : $_('rodecaster.recording.remoteHint')}
                    >
                      {#snippet trailing()}
                        {#if canPickRecordingDirectory}
                          <Button compact onclick={chooseRecordingDirectory}
                            >{$_('rodecaster.recording.choose')}</Button
                          >
                        {/if}
                      {/snippet}
                    </Field>
                    <Select
                      label={$_('rodecaster.recording.outputMode')}
                      value={rodecasterAudioRecording.outputMode}
                      options={[
                        { value: 'mainMix', label: $_('rodecaster.recording.mode.mainMix') },
                        { value: 'separate', label: $_('rodecaster.recording.mode.separate') },
                        {
                          value: 'combinedStereo',
                          label: $_('rodecaster.recording.mode.combinedStereo'),
                        },
                        {
                          value: 'multichannelFlac',
                          label: $_('rodecaster.recording.mode.multichannelFlac'),
                        },
                      ]}
                      onchange={(outputMode) =>
                        (rodecasterAudioRecording = {
                          ...rodecasterAudioRecording,
                          outputMode: outputMode as RodecasterAudioRecordingConfig['outputMode'],
                        })}
                    />
                    {#if rodecasterAudioRecording.outputMode !== 'mainMix'}
                      <Select
                        label={$_('rodecaster.recording.processingMode')}
                        value={rodecasterAudioRecording.processingMode}
                        options={[
                          {
                            value: 'preFader',
                            label: $_('rodecaster.recording.processing.preFader'),
                          },
                          {
                            value: 'preFaderBypass',
                            label: $_('rodecaster.recording.processing.preFaderBypass'),
                          },
                          {
                            value: 'postFader',
                            label: $_('rodecaster.recording.processing.postFader'),
                          },
                        ]}
                        onchange={(processingMode) =>
                          (rodecasterAudioRecording = {
                            ...rodecasterAudioRecording,
                            processingMode:
                              processingMode as RodecasterAudioRecordingConfig['processingMode'],
                          })}
                      />
                      <fieldset>
                        <legend>{$_('rodecaster.recording.sources')}</legend>
                        {#each rodecasterAudioSources as source (source.label)}
                          {@const identity = source.identity}
                          {#if identity.kind === 'faderSlot'}
                            <label class="recording-source">
                              <input
                                type="checkbox"
                                checked={recordingSlotSelected(identity.number)}
                                onchange={() => toggleRecordingSlot(identity.number)}
                              />
                              <span>Fader {identity.number} · {source.label}</span>
                            </label>
                          {/if}
                        {/each}
                        {#if recordingSourceSelectionMissing}
                          <p class="recording-error">
                            {rodecasterAudioSources.length
                              ? $_('rodecaster.recording.sourcesRequired')
                              : $_('rodecaster.recording.sourcesUnavailable')}
                          </p>
                        {/if}
                      </fieldset>
                    {/if}
                    {#if rodecasterAudioEndpoint}
                      <p class="recording-endpoint">
                        {rodecasterAudioEndpoint.sampleRate} Hz · {rodecasterAudioEndpoint.channels}
                        ch
                      </p>
                    {/if}
                    <Button
                      variant="secondary"
                      compact
                      disabled={recordingSourceSelectionMissing}
                      onclick={() => save(meta)}
                    >
                      {$_('rodecaster.recording.save')}
                    </Button>
                  </div>
                  <List>
                    <Row
                      title={$_('conn.rodecaster.mixer')}
                      meta={$_('conn.rodecaster.mixerMeta')}
                      detail={$_('conn.camera.open')}
                      href="/settings/connectors/rodecaster"
                      last
                    >
                      {#snippet icon()}<Glyph char="⇵" size={28} />{/snippet}
                    </Row>
                  </List>
                {/if}

                {#if meta.id === 'broadlink'}
                  <DiscoveryPanel
                    title={$_('conn.discovery.title')}
                    description={$_('conn.discovery.description')}
                    scanLabel={devices.length
                      ? $_('conn.discovery.scanAgain')
                      : $_('conn.discovery.scan')}
                    {scanning}
                    scanningLabel={$_('conn.discovery.scanning')}
                    onscan={scan}
                  >
                    {#if devices.length === 0}
                      <p class="empty">{$_('conn.discovery.empty')}</p>
                    {:else}
                      <ul class="devices">
                        {#each devices as d (d.id)}
                          <li>
                            <strong>{d.name}</strong><code>{d.host}</code><em
                              >{d.model ?? d.deviceType}</em
                            >
                          </li>
                        {/each}
                      </ul>
                    {/if}
                  </DiscoveryPanel>
                {/if}
              </section>
            {/if}
          {/each}
        </List>
      {/if}
    {/each}

    <p class="note">{$_('conn.note')}</p>
  </div>

  <aside class="ops-col">
    <section class="ops-card">
      <span class="kicker">{$_('conn.ops.kicker')}</span>
      <h2>{$_('conn.ops.route', { values: { dest: destination } })}</h2>
      <p class="route">
        <Dot color={obsConnected ? 'var(--status-ok)' : 'var(--status-off)'} size={6} />
        {obsConnected ? $_('conn.ops.obsReady') : $_('conn.ops.obsAttention')}
      </p>
      <div class="stats">
        <Stat label={$_('conn.ops.enabled')} value={enabledCount} unit={`/${CONNECTORS.length}`} />
        <Stat label={$_('conn.ops.ready')} value={readyCount} />
      </div>
      <dl>
        <div>
          <dt>{$_('conn.ops.rtmp')}</dt>
          <dd>{rtmp || '—'}</dd>
        </div>
        <div>
          <dt>{$_('conn.ops.deviceLayer')}</dt>
          <dd>
            {devices.length
              ? $_('conn.ops.devicesFound', { values: { n: devices.length } })
              : $_('conn.ops.awaitingScan')}
          </dd>
        </div>
        <div>
          <dt>{$_('conn.ops.authState')}</dt>
          <dd>{ytLinked ? $_('conn.ops.ytLinked') : $_('conn.ops.ytNeedsLogin')}</dd>
        </div>
      </dl>
    </section>
  </aside>
</div>
<div class="spacer"></div>

<LoginSheet bind:provider={loginProvider} />

<style>
  .overview {
    margin: 0 24px;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    border-block: 1px solid var(--border-hairline);
  }
  .connectors-workspace,
  .main-col {
    display: contents;
  }
  .ops-col {
    display: none;
  }
  .connector {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 24px;
    border-bottom: 1px solid var(--border-hairline);
  }
  .muted {
    opacity: 0.72;
  }
  .head {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 14px;
    background: transparent;
    border: 0;
    padding: 0;
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
    font-family: inherit;
  }
  .text {
    min-width: 0;
  }
  strong {
    display: block;
    font-size: 15px;
    font-weight: 500;
    color: var(--text-primary);
  }
  em {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-secondary);
    font-style: normal;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail {
    padding: 14px 24px 18px 72px;
    border-bottom: 1px solid var(--border-hairline);
    display: grid;
    /* An auto column takes its content's max-content width, which on a phone pushes
       the whole page sideways; this holds it to the column it was given. */
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
  }
  .detail p {
    margin: 0;
    font-family: var(--font-display);
    font-style: italic;
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1.45;
  }
  .recording-config {
    display: grid;
    gap: 12px;
    padding-block: 8px;
  }
  .recording-config h3 {
    margin: 0;
    color: var(--text-primary);
    font-size: 15px;
    font-weight: 500;
  }
  .detail .recording-help,
  .detail .recording-endpoint {
    font-family: var(--font-body);
    font-style: normal;
    font-size: 12px;
  }
  .detail .recording-error {
    font-family: var(--font-body);
    font-style: normal;
    font-size: 12px;
    color: var(--status-error);
  }
  .recording-config fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }
  .recording-warning {
    display: grid;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--status-warn);
  }
  .detail .recording-warning p {
    font-family: var(--font-body);
    font-style: normal;
    font-size: 12px;
  }
  .recording-config legend {
    margin-bottom: 8px;
    font-family: var(--font-label);
    font-size: 10px;
    letter-spacing: 1.4px;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .recording-source {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 36px;
    font-size: 13px;
    color: var(--text-primary);
  }
  .middlecontrol-devices {
    display: grid;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border-hairline);
    background: var(--surface-raised);
  }
  .middlecontrol-devices-head,
  .middlecontrol-devices li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .middlecontrol-devices-head h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 500;
    color: var(--text-primary);
  }
  .middlecontrol-devices-head > span {
    font-family: var(--font-label);
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 1px;
  }
  .middlecontrol-devices ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .middlecontrol-devices li {
    min-height: 38px;
    border-top: 1px solid var(--border-hairline);
  }
  .middlecontrol-device-name strong {
    font-size: 13px;
  }
  .middlecontrol-device-name em {
    margin-top: 2px;
    font-size: 11px;
  }
  .middlecontrol-badges {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 5px;
  }
  .middlecontrol-badges span {
    padding: 3px 6px;
    border: 1px solid var(--border-control);
    font-family: var(--font-label);
    font-size: 9px;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.7px;
  }
  .middlecontrol-badges .recording {
    border-color: var(--status-error);
    color: var(--status-error);
  }
  .middlecontrol-badges .ptz {
    border-color: var(--accent);
    color: var(--accent);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  /* The labels never wrap, so on a narrow column the pair stacks rather than
     pushing the page sideways. */
  .actions :global(button) {
    flex: 1 1 140px;
  }
  .empty {
    font-family: var(--font-display);
    font-style: italic;
    color: var(--text-muted);
    font-size: 14px;
  }
  .devices {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .devices li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 12px;
    padding: 8px 0;
    border-bottom: 1px solid var(--border-hairline);
  }
  .devices code {
    font-family: var(--font-label);
    font-size: 11px;
    color: var(--text-muted);
  }
  .devices em {
    font-size: 11px;
    color: var(--text-muted);
  }
  /* The 72px indent aligns the panel with the connector's name, which a phone
     cannot spare — a translated action label would spill out of its button. */
  @media (max-width: 480px) {
    .detail {
      padding-left: 24px;
    }
  }

  .note {
    padding: 28px 32px 12px;
    text-align: center;
    margin: 0;
    font-family: var(--font-display);
    font-style: italic;
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1.45;
  }
  .spacer {
    height: 120px;
  }
  .ops-card {
    background: var(--surface-raised);
    border-block: 1px solid var(--border-hairline);
    padding: 18px;
  }
  .kicker,
  dt {
    font-family: var(--font-label);
    font-size: 9px;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  h2 {
    margin: 8px 0 10px;
    font-family: var(--font-display);
    font-size: 28px;
    line-height: 1;
    font-weight: 500;
    color: var(--text-primary);
  }
  .route {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.4;
  }
  .stats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
    margin: 18px 0;
    padding-block: 16px;
    border-block: 1px solid var(--border-hairline);
  }
  dl,
  dd {
    margin: 0;
  }
  dl div {
    padding: 12px 0;
    border-bottom: 1px solid var(--border-hairline);
  }
  dl div:last-child {
    border-bottom: 0;
  }
  dd {
    margin-top: 5px;
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  @media (min-width: 760px) {
    .connectors-workspace {
      display: grid;
      grid-template-columns: minmax(286px, 1fr) minmax(206px, 260px);
      gap: 0 12px;
      align-items: start;
      padding: 0 14px 64px;
    }
    .main-col,
    .ops-col {
      display: block;
      min-width: 0;
    }
    .ops-col {
      position: sticky;
      top: 18px;
    }
    .overview {
      margin: 0;
    }
    .connector {
      padding-inline: 18px;
    }
    .detail {
      padding-left: 18px;
      padding-right: 18px;
    }
    .note {
      padding-bottom: 0;
    }
    .spacer {
      height: 0;
    }
  }
  @media (min-width: 1360px) {
    .connectors-workspace {
      grid-template-columns: minmax(620px, 1fr) 360px;
      gap: 0 30px;
      padding-inline: 32px;
    }
    .detail {
      padding-left: 72px;
      padding-right: 24px;
    }
  }
</style>
