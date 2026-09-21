<script lang="ts">
  import { openExternal } from '@metocast/core-client';
  import { _ } from 'svelte-i18n';
  import { toast } from 'svelte-sonner';
  import { appMode } from '$lib/stores/mode.js';
  import type { Writable } from 'svelte/store';
  import {
    obsConfig,
    obsStatus,
    vmixConfig,
    vmixStatus,
    atemConfig,
    atemStatus,
    middlecontrolConfig,
    middlecontrolStatus,
    middlecontrolState,
    broadlinkConfig,
    broadlinkStatus,
    youtubeConfig,
    youtubeStatus,
    facebookConfig,
    facebookStatus,
    discordConfig,
    discordStatus,
    szentirasConfig,
    szentirasStatus,
  } from '$lib/stores/connectors.js';
  import type {
    ObsConfig,
    VmixConfig,
    AtemConfig,
    MiddlecontrolConfig,
    BroadlinkConfig,
    YouTubeConfig,
    FacebookConfig,
    DiscordConfig,
    SzentirasConfig,
  } from '$lib/stores/connectors.js';
  import { findConnector } from '$lib/connectors/registry.js';
  import ConnectorStatusBadge from './ConnectorStatusBadge.svelte';
  import {
    youtubeLogout,
    facebookLogout,
    fetchYouTubeStreamKey,
    fetchFacebookStreamKey,
    saveConnectorConfig,
    connectObs as connectObsRequest,
    disconnectObs as disconnectObsRequest,
    fetchObsStreamSettings,
    applyObsStreamSettings,
    youtubeAuthUrl,
    facebookAuthUrl,
    revealConnectorSecrets,
    discoverMiddlecontrol,
  } from '@metocast/core-client';
  import type { DiscoveredMiddlecontrol } from '@metocast/core-client';
  import type {
    ConnectorConfigMap,
    ConnectorName,
    ObsStreamSettings,
  } from '@metocast/core-client/schemas/connectors';
  import BroadlinkDiscoveryPanel from './broadlink/DiscoveryPanel.svelte';

  const MIDDLECONTROL_SDK_URL =
    'https://www.middlethings.co/support/docs/joysticks-and-controllers/external-sdk-api/';

  interface Props {
    connectorId: string;
    onSaveSuccess?: () => void;
  }

  let { connectorId, onSaveSuccess }: Props = $props();

  const def = $derived(findConnector(connectorId));

  // Stored secrets are readable only on the machine hosting the server, and only
  // through Tauri IPC — a client-mode window is talking to someone else's core.
  const isHost = $derived(
    $appMode === 'server' &&
      typeof window !== 'undefined' &&
      typeof (window as Window & { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ !==
        'undefined',
  );

  async function revealSecret<K extends ConnectorName>(
    name: K,
    apply: (config: ConnectorConfigMap[K]) => void,
  ) {
    try {
      apply(await revealConnectorSecrets(name));
    } catch (e) {
      toast.error($_('appSettings.connectors.revealFailed'), { description: String(e) });
    }
  }

  async function persistConfig<K extends ConnectorName>(
    name: K,
    form: ConnectorConfigMap[K],
    store: Writable<ConnectorConfigMap[K]>,
  ): Promise<string> {
    const connector = def?.name ?? name;
    try {
      await saveConnectorConfig(name, form);
      store.set({ ...form });
      toast.success($_('appSettings.connectors.saved', { values: { connector } }));
      onSaveSuccess?.();
      return '';
    } catch (e) {
      const message = String(e);
      toast.error($_('appSettings.connectors.saveFailed', { values: { connector } }), {
        description: message,
      });
      return message;
    }
  }

  // ── OBS ────────────────────────────────────────────────────────────────────
  let obsForm: ObsConfig = $state({
    enabled: false,
    host: 'localhost',
    port: 4455,
    password: null,
  });
  let obsSaving = $state(false);
  let obsError = $state('');

  // ── OBS Streaming Destination ─────────────────────────────────────────────
  type ObsDestination = 'youtube' | 'facebook';
  let obsDestination = $state<ObsDestination>('youtube');
  let obsStreamSettings = $state<ObsStreamSettings | null>(null);
  let obsDestSaving = $state(false);
  let obsDestError = $state('');
  let obsDestYtUrl = $state('');
  let obsDestFbUrl = $state('');
  let obsDestFetchingYt = $state(false);
  let obsDestFetchingFb = $state(false);

  $effect(() => {
    if (connectorId !== 'obs') return;
    if ($obsStatus === 'connected') {
      fetchObsStreamSettings()
        .then((s) => {
          obsStreamSettings = s;
          const srv = s.server.toLowerCase();
          if (srv.includes('youtube')) {
            obsDestination = 'youtube';
          } else if (srv.includes('facebook') || srv.includes('fbcdn')) {
            obsDestination = 'facebook';
          }
        })
        .catch(() => {
          obsStreamSettings = null;
        });
    } else {
      obsStreamSettings = null;
    }
  });

  async function fetchDestYouTubeKey() {
    obsDestFetchingYt = true;
    obsDestError = '';
    try {
      const result = await fetchYouTubeStreamKey();
      obsDestYtUrl = result.rtmpUrl;
    } catch (e) {
      obsDestError = String(e);
    } finally {
      obsDestFetchingYt = false;
    }
  }

  async function fetchDestFacebookKey() {
    obsDestFetchingFb = true;
    obsDestError = '';
    try {
      const result = await fetchFacebookStreamKey();
      obsDestFbUrl = result.rtmpUrl;
    } catch (e) {
      obsDestError = String(e);
    } finally {
      obsDestFetchingFb = false;
    }
  }

  async function applyObsDestination() {
    obsDestSaving = true;
    obsDestError = '';
    try {
      let server = '';
      let key = '';
      if (obsDestination === 'youtube') {
        const lastSlash = obsDestYtUrl.lastIndexOf('/');
        server = lastSlash > 6 ? obsDestYtUrl.slice(0, lastSlash) : obsDestYtUrl;
        key = lastSlash > 6 ? obsDestYtUrl.slice(lastSlash + 1) : '';
      } else if (obsDestination === 'facebook') {
        const lastSlash = obsDestFbUrl.lastIndexOf('/');
        server = lastSlash > 6 ? obsDestFbUrl.slice(0, lastSlash) : obsDestFbUrl;
        key = lastSlash > 6 ? obsDestFbUrl.slice(lastSlash + 1) : '';
      }
      await applyObsStreamSettings(server, key);
      // Refresh displayed settings
      obsStreamSettings = await fetchObsStreamSettings();
    } catch (e) {
      obsDestError = String(e);
    } finally {
      obsDestSaving = false;
    }
  }

  $effect(() => {
    if (connectorId === 'obs') obsForm = { ...$obsConfig };
  });

  async function saveObs() {
    obsSaving = true;
    obsError = await persistConfig('obs', obsForm, obsConfig);
    obsSaving = false;
  }

  async function connectObs() {
    obsError = '';
    try {
      await connectObsRequest();
    } catch (e) {
      obsError = String(e);
    }
  }

  async function disconnectObs() {
    obsError = '';
    try {
      await disconnectObsRequest();
    } catch (e) {
      obsError = String(e);
    }
  }

  // ── VMix ───────────────────────────────────────────────────────────────────
  let vmixForm: VmixConfig = $state({ enabled: false, host: 'localhost', port: 8088 });
  let vmixSaving = $state(false);
  let vmixError = $state('');

  $effect(() => {
    if (connectorId === 'vmix') vmixForm = { ...$vmixConfig };
  });

  async function saveVmix() {
    vmixSaving = true;
    vmixError = await persistConfig('vmix', vmixForm, vmixConfig);
    vmixSaving = false;
  }

  // ── ATEM ───────────────────────────────────────────────────────────────────
  let atemForm: AtemConfig = $state({ enabled: false, host: '', port: 9910 });
  let atemSaving = $state(false);
  let atemError = $state('');

  $effect(() => {
    if (connectorId === 'atem') atemForm = { ...$atemConfig };
  });

  async function saveAtem() {
    atemSaving = true;
    atemError = await persistConfig('atem', atemForm, atemConfig);
    atemSaving = false;
  }

  // ── Middle Control ────────────────────────────────────────────────────────
  let middlecontrolForm: MiddlecontrolConfig = $state({ enabled: false, host: '', port: 11584 });
  let middlecontrolSaving = $state(false);
  let middlecontrolError = $state('');
  let middlecontrolDevices = $state<DiscoveredMiddlecontrol[]>([]);
  let middlecontrolScanning = $state(false);
  let middlecontrolScanned = $state(false);
  const middlecontrolCameraIds = $derived($middlecontrolState.connectedCameraIds ?? []);
  const middlecontrolApcrIds = $derived($middlecontrolState.connectedApcrIds ?? []);
  const middlecontrolRecordingIds = $derived($middlecontrolState.recordingCameraIds ?? []);
  const middlecontrolDeviceIds = $derived(
    [...new Set([...middlecontrolCameraIds, ...middlecontrolApcrIds])].sort((a, b) => a - b),
  );

  $effect(() => {
    if (connectorId === 'middlecontrol') middlecontrolForm = { ...$middlecontrolConfig };
  });

  async function saveMiddlecontrol() {
    middlecontrolSaving = true;
    middlecontrolError = await persistConfig(
      'middlecontrol',
      middlecontrolForm,
      middlecontrolConfig,
    );
    middlecontrolSaving = false;
  }

  async function scanMiddlecontrol() {
    middlecontrolScanning = true;
    middlecontrolError = '';
    try {
      middlecontrolDevices = await discoverMiddlecontrol();
      middlecontrolScanned = true;
    } catch (e) {
      middlecontrolError = String(e);
    } finally {
      middlecontrolScanning = false;
    }
  }

  // ── YouTube ────────────────────────────────────────────────────────────────
  let ytForm: YouTubeConfig = $state({ enabled: false, clientId: '', clientSecret: '' });
  let ytSaving = $state(false);
  let ytError = $state('');
  let ytLoggingIn = $state(false);

  $effect(() => {
    if (connectorId === 'youtube') ytForm = { ...$youtubeConfig };
  });

  async function saveYt() {
    ytSaving = true;
    ytError = await persistConfig('youtube', ytForm, youtubeConfig);
    ytSaving = false;
  }

  async function loginYt() {
    ytLoggingIn = true;
    ytError = '';
    try {
      await openExternal(await youtubeAuthUrl());
    } catch (e) {
      ytError = String(e);
    } finally {
      ytLoggingIn = false;
    }
  }

  async function logoutYt() {
    ytError = '';
    try {
      await youtubeLogout();
    } catch (e) {
      ytError = String(e);
    }
  }

  // ── Facebook ───────────────────────────────────────────────────────────────
  let fbForm: FacebookConfig = $state({ enabled: false, appId: '', appSecret: '', pageId: '' });
  let fbSaving = $state(false);
  let fbError = $state('');
  let fbLoggingIn = $state(false);

  $effect(() => {
    if (connectorId === 'facebook') fbForm = { ...$facebookConfig };
  });

  async function saveFb() {
    fbSaving = true;
    fbError = await persistConfig('facebook', fbForm, facebookConfig);
    fbSaving = false;
  }

  async function loginFb() {
    fbLoggingIn = true;
    fbError = '';
    try {
      await openExternal(await facebookAuthUrl());
    } catch (e) {
      fbError = String(e);
    } finally {
      fbLoggingIn = false;
    }
  }

  async function logoutFb() {
    fbError = '';
    try {
      await facebookLogout();
    } catch (e) {
      fbError = String(e);
    }
  }

  // ── Discord ────────────────────────────────────────────────────────────────
  let discordForm: DiscordConfig = $state({ enabled: false, webhookUrl: '' });
  let discordSaving = $state(false);
  let discordError = $state('');

  $effect(() => {
    if (connectorId === 'discord') discordForm = { ...$discordConfig };
  });

  async function saveDiscord() {
    discordSaving = true;
    discordError = await persistConfig('discord', discordForm, discordConfig);
    discordSaving = false;
  }

  // ── Broadlink ──────────────────────────────────────────────────────────────
  let broadlinkForm: BroadlinkConfig = $state({ enabled: false });
  let broadlinkSaving = $state(false);
  let broadlinkError = $state('');

  $effect(() => {
    if (connectorId === 'broadlink') broadlinkForm = { ...$broadlinkConfig };
  });

  async function saveBroadlink() {
    broadlinkSaving = true;
    broadlinkError = await persistConfig('broadlink', broadlinkForm, broadlinkConfig);
    broadlinkSaving = false;
  }

  // ── Szentírás.eu ───────────────────────────────────────────────────────────
  let szentirasForm: SzentirasConfig = $state({ enabled: false, apiKey: '' });
  let szentirasSaving = $state(false);
  let szentirasError = $state('');

  $effect(() => {
    if (connectorId === 'szentiras') szentirasForm = { ...$szentirasConfig };
  });

  async function saveSzentiras() {
    szentirasSaving = true;
    szentirasError = await persistConfig('szentiras', szentirasForm, szentirasConfig);
    szentirasSaving = false;
  }
</script>

{#snippet storedSecret(isSet: boolean | undefined, clear: () => void, reveal: () => void)}
  {#if isSet}
    <p class="note">
      {$_('appSettings.connectors.secretStored')}
      {#if isHost}
        <button type="button" class="btn-link" onclick={reveal}>
          {$_('appSettings.connectors.showSecret')}
        </button>
      {/if}
      <button type="button" class="btn-link" onclick={clear}>
        {$_('appSettings.connectors.clearSecret')}
      </button>
    </p>
  {/if}
{/snippet}

{#if def}
  <div class="settings-block">
    <!-- Header -->
    <div class="connector-header">
      <div>
        <h3>{def.name}</h3>
        {#if connectorId === 'obs'}
          <p class="note">{$_('appSettings.connectors.obs.subtitle')}</p>
        {:else if connectorId === 'vmix'}
          <p class="note">{$_('appSettings.connectors.vmix.subtitle')}</p>
        {:else if connectorId === 'atem'}
          <p class="note">{$_('appSettings.connectors.atem.subtitle')}</p>
        {:else if connectorId === 'middlecontrol'}
          <p class="note">{$_('appSettings.connectors.middlecontrol.subtitle')}</p>
        {:else if connectorId === 'broadlink'}
          <p class="note">{$_('appSettings.connectors.broadlink.subtitle')}</p>
        {:else if connectorId === 'youtube'}
          <p class="note">{$_('appSettings.connectors.youtube.subtitle')}</p>
        {:else if connectorId === 'facebook'}
          <p class="note">{$_('appSettings.connectors.facebook.subtitle')}</p>
        {:else if connectorId === 'discord'}
          <p class="note">{$_('appSettings.connectors.discord.subtitle')}</p>
        {:else if connectorId === 'szentiras'}
          <p class="note">{$_('appSettings.connectors.szentiras.subtitle')}</p>
        {/if}
      </div>
      {#if connectorId === 'obs'}
        <ConnectorStatusBadge name="OBS" status={$obsStatus} />
      {:else if connectorId === 'vmix'}
        <ConnectorStatusBadge name="VMix" status={$vmixStatus} />
      {:else if connectorId === 'atem'}
        <ConnectorStatusBadge name="ATEM" status={$atemStatus} />
      {:else if connectorId === 'middlecontrol'}
        <ConnectorStatusBadge name="Middle Control" status={$middlecontrolStatus} />
      {:else if connectorId === 'broadlink'}
        <ConnectorStatusBadge name="BroadLink" status={$broadlinkStatus} />
      {:else if connectorId === 'youtube'}
        <ConnectorStatusBadge name="YouTube" status={$youtubeStatus} />
      {:else if connectorId === 'facebook'}
        <ConnectorStatusBadge name="Facebook" status={$facebookStatus} />
      {:else if connectorId === 'discord'}
        <ConnectorStatusBadge name="Discord" status={$discordStatus} />
      {:else if connectorId === 'szentiras'}
        <ConnectorStatusBadge name="Szentírás.eu" status={$szentirasStatus} />
      {/if}
    </div>

    <!-- ── OBS form ──────────────────────────────────────────────────────── -->
    {#if connectorId === 'obs'}
      <div class="form-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={obsForm.enabled} />
          {$_('appSettings.connectors.obs.enabled')}
        </label>
      </div>

      <div class="form-grid">
        <div class="field">
          <label for="obs-host">{$_('appSettings.connectors.obs.host')}</label>
          <input id="obs-host" type="text" bind:value={obsForm.host} />
        </div>
        <div class="field">
          <label for="obs-port">{$_('appSettings.connectors.obs.port')}</label>
          <input id="obs-port" type="number" min="1" max="65535" bind:value={obsForm.port} />
        </div>
        <div class="field field--full">
          <label for="obs-password">{$_('appSettings.connectors.obs.password')}</label>
          <input
            id="obs-password"
            type="password"
            placeholder={$_('appSettings.connectors.obs.passwordPlaceholder')}
            value={obsForm.password ?? ''}
            oninput={(e) => {
              const val = (e.currentTarget as HTMLInputElement).value;
              obsForm.password = val.length > 0 ? val : null;
            }}
          />
          {@render storedSecret(
            $obsConfig.passwordSet,
            () => {
              obsForm.password = null;
              obsForm.passwordSet = false;
            },
            () => revealSecret('obs', (c) => (obsForm.password = c.password)),
          )}
        </div>
      </div>

      {#if obsError}
        <p class="error" role="alert">{obsError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveObs} disabled={obsSaving}>
          {obsSaving
            ? $_('appSettings.connectors.obs.saving')
            : $_('appSettings.connectors.obs.save')}
        </button>
        {#if $obsStatus === 'disconnected' || $obsStatus === 'error'}
          <button class="btn-secondary" onclick={connectObs}>
            {$_('appSettings.connectors.obs.connect')}
          </button>
        {:else if $obsStatus === 'connected' || $obsStatus === 'connecting'}
          <button class="btn-danger" onclick={disconnectObs}>
            {$_('appSettings.connectors.obs.disconnect')}
          </button>
        {/if}
      </div>

      <!-- ── Stream Preview ────────────────────────────────────────────── -->
      <!-- OBS Streaming Destination -->
      <div class="preview-section">
        <h4 class="preview-heading">OBS Streaming Destination</h4>

        {#if obsStreamSettings}
          <p class="rtmp-label">
            Current server: <code class="inline-code">{obsStreamSettings.server || '(none)'}</code>
          </p>
        {:else}
          <p class="rtmp-label">Connect OBS to view and change stream destination.</p>
        {/if}

        <fieldset class="dest-cards" disabled={$obsStatus !== 'connected'}>
          <legend class="sr-only">Streaming destination</legend>

          <label class="dest-card" class:dest-card--active={obsDestination === 'youtube'}>
            <input type="radio" name="obs-dest" value="youtube" bind:group={obsDestination} />
            <span class="dest-card-title">YouTube</span>
            <span class="dest-card-desc">Stream directly to YouTube Live</span>
            {#if obsDestination === 'youtube'}
              <div class="dest-url-row">
                <input
                  type="text"
                  class="dest-url-input"
                  placeholder="rtmp://a.rtmp.youtube.com/live2/…"
                  bind:value={obsDestYtUrl}
                  aria-label="YouTube RTMP URL"
                />
                <button
                  class="btn-fetch"
                  onclick={fetchDestYouTubeKey}
                  disabled={obsDestFetchingYt || $youtubeStatus !== 'connected'}
                  title={$youtubeStatus !== 'connected'
                    ? 'Connect YouTube first'
                    : 'Fetch stream key from YouTube API'}
                >
                  {obsDestFetchingYt ? 'Fetching…' : 'Fetch'}
                </button>
              </div>
            {/if}
          </label>

          <label class="dest-card" class:dest-card--active={obsDestination === 'facebook'}>
            <input type="radio" name="obs-dest" value="facebook" bind:group={obsDestination} />
            <span class="dest-card-title">Facebook</span>
            <span class="dest-card-desc">Stream directly to Facebook Live</span>
            {#if obsDestination === 'facebook'}
              <div class="dest-url-row">
                <input
                  type="text"
                  class="dest-url-input"
                  placeholder="rtmps://live-api-s.facebook.com:443/rtmp/…"
                  bind:value={obsDestFbUrl}
                  aria-label="Facebook RTMP URL"
                />
                <button
                  class="btn-fetch"
                  onclick={fetchDestFacebookKey}
                  disabled={obsDestFetchingFb || $facebookStatus !== 'connected'}
                  title={$facebookStatus !== 'connected'
                    ? 'Connect Facebook first'
                    : 'Fetch stream key from Facebook API'}
                >
                  {obsDestFetchingFb ? 'Fetching…' : 'Fetch'}
                </button>
              </div>
            {/if}
          </label>
        </fieldset>

        {#if obsDestError}
          <p class="error" role="alert">{obsDestError}</p>
        {/if}

        <button
          class="btn-primary"
          onclick={applyObsDestination}
          disabled={obsDestSaving || $obsStatus !== 'connected'}
        >
          {obsDestSaving ? 'Applying…' : 'Apply to OBS'}
        </button>
      </div>

      <a href="/obs-devices" class="device-monitor-link">{$_('obsDevices.manageDeviceMonitors')}</a>

      <!-- ── VMix form ─────────────────────────────────────────────────────── -->
    {:else if connectorId === 'vmix'}
      <p class="coming-soon-notice">{$_('appSettings.connectors.vmix.comingSoon')}</p>

      <fieldset disabled>
        <div class="form-row">
          <label class="checkbox-label">
            <input type="checkbox" bind:checked={vmixForm.enabled} />
            {$_('appSettings.connectors.vmix.enabled')}
          </label>
        </div>
        <div class="form-grid">
          <div class="field">
            <label for="vmix-host">{$_('appSettings.connectors.vmix.host')}</label>
            <input id="vmix-host" type="text" bind:value={vmixForm.host} />
          </div>
          <div class="field">
            <label for="vmix-port">{$_('appSettings.connectors.vmix.port')}</label>
            <input id="vmix-port" type="number" min="1" max="65535" bind:value={vmixForm.port} />
          </div>
        </div>
      </fieldset>

      {#if vmixError}
        <p class="error" role="alert">{vmixError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveVmix} disabled={vmixSaving}>
          {vmixSaving
            ? $_('appSettings.connectors.vmix.saving')
            : $_('appSettings.connectors.vmix.save')}
        </button>
      </div>

      <!-- ── Middle Control form ──────────────────────────────────────────── -->
    {:else if connectorId === 'middlecontrol'}
      <div class="form-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={middlecontrolForm.enabled} />
          {$_('appSettings.connectors.middlecontrol.enabled')}
        </label>
      </div>
      <div class="form-grid">
        <div class="field">
          <label for="middlecontrol-host">{$_('appSettings.connectors.middlecontrol.host')}</label>
          <input
            id="middlecontrol-host"
            type="text"
            placeholder="localhost"
            bind:value={middlecontrolForm.host}
          />
        </div>
        <div class="field">
          <label for="middlecontrol-port">{$_('appSettings.connectors.middlecontrol.port')}</label>
          <input
            id="middlecontrol-port"
            type="number"
            min="1"
            max="65535"
            bind:value={middlecontrolForm.port}
          />
        </div>
      </div>
      <p class="note">{$_('appSettings.connectors.middlecontrol.help')}</p>

      <button
        class="btn-link middlecontrol-sdk-link"
        type="button"
        onclick={() => void openExternal(MIDDLECONTROL_SDK_URL)}
      >
        {$_('appSettings.connectors.middlecontrol.sdk')} ↗
      </button>

      <section class="middlecontrol-devices" aria-labelledby="middlecontrol-devices-title">
        <div class="middlecontrol-devices-head">
          <h4 id="middlecontrol-devices-title">
            {$_('appSettings.connectors.middlecontrol.devicesTitle')}
          </h4>
          <span>
            {$_('appSettings.connectors.middlecontrol.devicesSummary', {
              values: {
                cameras: middlecontrolCameraIds.length,
                ptz: middlecontrolApcrIds.length,
              },
            })}
          </span>
        </div>
        {#if $middlecontrolStatus !== 'connected'}
          <p class="note">
            {$_('appSettings.connectors.middlecontrol.devicesDisconnected')}
          </p>
        {:else if middlecontrolDeviceIds.length === 0}
          <p class="note">{$_('appSettings.connectors.middlecontrol.devicesEmpty')}</p>
        {:else}
          <ul>
            {#each middlecontrolDeviceIds as id (id)}
              <li>
                <span>
                  <strong>
                    {$_('appSettings.connectors.middlecontrol.camera', { values: { id } })}
                  </strong>
                  <small>
                    {middlecontrolCameraIds.includes(id)
                      ? $_('appSettings.connectors.middlecontrol.connected')
                      : $_('appSettings.connectors.middlecontrol.cameraOffline')}
                  </small>
                </span>
                <span class="middlecontrol-badges">
                  {#if $middlecontrolState.selectedCamera === id}
                    <span>{$_('appSettings.connectors.middlecontrol.selected')}</span>
                  {/if}
                  {#if middlecontrolRecordingIds.includes(id)}
                    <span class="recording">
                      {$_('appSettings.connectors.middlecontrol.recording')}
                    </span>
                  {/if}
                  {#if middlecontrolApcrIds.includes(id)}
                    <span class="ptz">{$_('appSettings.connectors.middlecontrol.ptz')}</span>
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <div class="button-row discovery-actions">
        <button
          class="btn-secondary"
          type="button"
          onclick={() => (middlecontrolForm.host = 'localhost')}
        >
          {$_('appSettings.connectors.middlecontrol.useLocal')}
        </button>
        <button
          class="btn-secondary"
          type="button"
          onclick={scanMiddlecontrol}
          disabled={middlecontrolScanning}
        >
          {middlecontrolScanning
            ? $_('appSettings.connectors.middlecontrol.scanning')
            : middlecontrolScanned
              ? $_('appSettings.connectors.middlecontrol.scanAgain')
              : $_('appSettings.connectors.middlecontrol.scan')}
        </button>
      </div>
      <p class="note">{$_('appSettings.connectors.middlecontrol.discoveryDescription')}</p>
      {#if middlecontrolScanned && middlecontrolDevices.length === 0}
        <p class="note">{$_('appSettings.connectors.middlecontrol.discoveryEmpty')}</p>
      {:else if middlecontrolDevices.length > 0}
        <div class="button-row">
          {#each middlecontrolDevices as device (`${device.host}:${device.port}`)}
            <button
              class="btn-secondary"
              type="button"
              onclick={() => {
                middlecontrolForm.host = device.host;
                middlecontrolForm.port = device.port;
              }}
            >
              {$_('appSettings.connectors.middlecontrol.useEndpoint', {
                values: { address: `${device.host}:${device.port}` },
              })}
            </button>
          {/each}
        </div>
      {/if}

      {#if middlecontrolError}
        <p class="error" role="alert">{middlecontrolError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveMiddlecontrol} disabled={middlecontrolSaving}>
          {middlecontrolSaving
            ? $_('appSettings.connectors.middlecontrol.saving')
            : $_('appSettings.connectors.middlecontrol.save')}
        </button>
      </div>

      <!-- ── ATEM form ─────────────────────────────────────────────────────── -->
    {:else if connectorId === 'atem'}
      <fieldset>
        <div class="form-row">
          <label class="checkbox-label">
            <input type="checkbox" bind:checked={atemForm.enabled} />
            {$_('appSettings.connectors.atem.enabled')}
          </label>
        </div>
        <div class="form-grid">
          <div class="field">
            <label for="atem-host">{$_('appSettings.connectors.atem.host')}</label>
            <input id="atem-host" type="text" bind:value={atemForm.host} />
          </div>
          <div class="field">
            <label for="atem-port">{$_('appSettings.connectors.atem.port')}</label>
            <input id="atem-port" type="number" min="1" max="65535" bind:value={atemForm.port} />
          </div>
        </div>
      </fieldset>

      {#if atemError}
        <p class="error" role="alert">{atemError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveAtem} disabled={atemSaving}>
          {atemSaving
            ? $_('appSettings.connectors.atem.saving')
            : $_('appSettings.connectors.atem.save')}
        </button>
      </div>

      <!-- ── YouTube form ───────────────────────────────────────────────────── -->
    {:else if connectorId === 'youtube'}
      <div class="form-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={ytForm.enabled} />
          {$_('appSettings.connectors.youtube.enabled')}
        </label>
      </div>

      <div class="form-grid">
        <div class="field">
          <label for="yt-client-id">{$_('appSettings.connectors.youtube.clientId')}</label>
          <input id="yt-client-id" type="text" bind:value={ytForm.clientId} />
        </div>
        <div class="field">
          <label for="yt-client-secret">{$_('appSettings.connectors.youtube.clientSecret')}</label>
          <input id="yt-client-secret" type="password" bind:value={ytForm.clientSecret} />
          {@render storedSecret(
            $youtubeConfig.clientSecretSet,
            () => {
              ytForm.clientSecret = '';
              ytForm.clientSecretSet = false;
            },
            () => revealSecret('youtube', (c) => (ytForm.clientSecret = c.clientSecret)),
          )}
        </div>
      </div>

      {#if ytError}
        <p class="error" role="alert">{ytError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveYt} disabled={ytSaving}>
          {ytSaving
            ? $_('appSettings.connectors.youtube.saving')
            : $_('appSettings.connectors.youtube.save')}
        </button>
        {#if $youtubeStatus === 'disconnected' || $youtubeStatus === 'error'}
          <button class="btn-secondary" onclick={loginYt} disabled={ytLoggingIn}>
            {ytLoggingIn
              ? $_('appSettings.connectors.youtube.loggingIn')
              : $_('appSettings.connectors.youtube.login')}
          </button>
        {:else if $youtubeStatus === 'connected'}
          <button class="btn-danger" onclick={logoutYt}>
            {$_('appSettings.connectors.youtube.logout')}
          </button>
        {/if}
      </div>

      <!-- ── Facebook form ──────────────────────────────────────────────────── -->
    {:else if connectorId === 'facebook'}
      <div class="form-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={fbForm.enabled} />
          {$_('appSettings.connectors.facebook.enabled')}
        </label>
      </div>

      <div class="form-grid">
        <div class="field">
          <label for="fb-app-id">{$_('appSettings.connectors.facebook.appId')}</label>
          <input id="fb-app-id" type="text" bind:value={fbForm.appId} />
        </div>
        <div class="field">
          <label for="fb-app-secret">{$_('appSettings.connectors.facebook.appSecret')}</label>
          <input id="fb-app-secret" type="password" bind:value={fbForm.appSecret} />
          {@render storedSecret(
            $facebookConfig.appSecretSet,
            () => {
              fbForm.appSecret = '';
              fbForm.appSecretSet = false;
            },
            () => revealSecret('facebook', (c) => (fbForm.appSecret = c.appSecret)),
          )}
        </div>
        <div class="field field--full">
          <label for="fb-page-id">{$_('appSettings.connectors.facebook.pageId')}</label>
          <input id="fb-page-id" type="text" bind:value={fbForm.pageId} />
        </div>
      </div>

      {#if fbError}
        <p class="error" role="alert">{fbError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveFb} disabled={fbSaving}>
          {fbSaving
            ? $_('appSettings.connectors.facebook.saving')
            : $_('appSettings.connectors.facebook.save')}
        </button>
        {#if $facebookStatus === 'disconnected' || $facebookStatus === 'error'}
          <button class="btn-secondary" onclick={loginFb} disabled={fbLoggingIn}>
            {fbLoggingIn
              ? $_('appSettings.connectors.facebook.loggingIn')
              : $_('appSettings.connectors.facebook.login')}
          </button>
        {:else if $facebookStatus === 'connected'}
          <button class="btn-danger" onclick={logoutFb}>
            {$_('appSettings.connectors.facebook.logout')}
          </button>
        {/if}
      </div>

      <!-- ── BroadLink form ────────────────────────────────────────────────── -->
    {:else if connectorId === 'broadlink'}
      <div class="form-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={broadlinkForm.enabled} />
          {$_('appSettings.connectors.broadlink.enabled')}
        </label>
      </div>

      {#if broadlinkError}
        <p class="error" role="alert">{broadlinkError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveBroadlink} disabled={broadlinkSaving}>
          {broadlinkSaving
            ? $_('appSettings.connectors.broadlink.saving')
            : $_('appSettings.connectors.broadlink.save')}
        </button>
      </div>

      <BroadlinkDiscoveryPanel />

      <!-- ── Discord form ───────────────────────────────────────────────────── -->
    {:else if connectorId === 'discord'}
      <p class="coming-soon-notice">{$_('appSettings.connectors.discord.comingSoon')}</p>

      <fieldset disabled>
        <div class="form-row">
          <label class="checkbox-label">
            <input type="checkbox" bind:checked={discordForm.enabled} />
            {$_('appSettings.connectors.discord.enabled')}
          </label>
        </div>
        <div class="form-grid">
          <div class="field field--full">
            <label for="discord-webhook">{$_('appSettings.connectors.discord.webhookUrl')}</label>
            <input id="discord-webhook" type="text" bind:value={discordForm.webhookUrl} />
            {@render storedSecret(
              $discordConfig.webhookUrlSet,
              () => {
                discordForm.webhookUrl = '';
                discordForm.webhookUrlSet = false;
              },
              () => revealSecret('discord', (c) => (discordForm.webhookUrl = c.webhookUrl)),
            )}
          </div>
        </div>
      </fieldset>

      {#if discordError}
        <p class="error" role="alert">{discordError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveDiscord} disabled={discordSaving}>
          {discordSaving
            ? $_('appSettings.connectors.discord.saving')
            : $_('appSettings.connectors.discord.save')}
        </button>
      </div>

      <!-- ── Szentírás.eu form ──────────────────────────────────────────────── -->
    {:else if connectorId === 'szentiras'}
      <div class="form-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={szentirasForm.enabled} />
          {$_('appSettings.connectors.szentiras.enabled')}
        </label>
      </div>
      <div class="form-grid">
        <div class="field field--full">
          <label for="szentiras-api-key">{$_('appSettings.connectors.szentiras.apiKey')}</label>
          <input
            id="szentiras-api-key"
            type="password"
            autocomplete="off"
            bind:value={szentirasForm.apiKey}
          />
          {@render storedSecret(
            $szentirasConfig.apiKeySet,
            () => {
              szentirasForm.apiKey = '';
              szentirasForm.apiKeySet = false;
            },
            () => revealSecret('szentiras', (c) => (szentirasForm.apiKey = c.apiKey)),
          )}
          <p class="note">
            {$_('appSettings.connectors.szentiras.apiKeyHint')}
            <a href="https://szentiras.eu/profile/api-keys" target="_blank" rel="noreferrer">
              szentiras.eu/profile/api-keys
            </a>
          </p>
        </div>
      </div>

      {#if szentirasError}
        <p class="error" role="alert">{szentirasError}</p>
      {/if}

      <div class="button-row">
        <button class="btn-primary" onclick={saveSzentiras} disabled={szentirasSaving}>
          {szentirasSaving
            ? $_('appSettings.connectors.szentiras.saving')
            : $_('appSettings.connectors.szentiras.save')}
        </button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .settings-block {
    padding: 1.25rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  /* ── Stream Preview section ──────────────────────────────────────────────── */
  .preview-section {
    margin-top: 1.25rem;
    padding-top: 1.25rem;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .preview-heading {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--text-primary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .rtmp-label {
    margin: 0;
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .btn-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent, #2563eb);
    text-decoration: underline;
    cursor: pointer;
  }

  .btn-fetch {
    padding: 0.375rem 0.75rem;
    background: var(--content-bg);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.8125rem;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .btn-fetch:hover:not(:disabled) {
    background: var(--nav-item-hover);
  }

  .btn-fetch:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* ── OBS Destination cards ──────────────────────────────────────────────── */
  .dest-cards {
    border: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .dest-cards:disabled .dest-card {
    opacity: 0.5;
    cursor: not-allowed;
    pointer-events: none;
  }

  .dest-card {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.625rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    cursor: pointer;
    transition:
      border-color 0.15s,
      background 0.15s;
  }

  .dest-card input[type='radio'] {
    position: absolute;
    opacity: 0;
    pointer-events: none;
    width: 0;
    height: 0;
  }

  .dest-card--active {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .dest-card:hover:not(.dest-card--active) {
    border-color: var(--border);
    background: var(--content-bg);
  }

  .dest-card-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .dest-card--active .dest-card-title {
    color: var(--accent);
  }

  .dest-card-desc {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .dest-url-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-top: 0.375rem;
  }

  .dest-url-input {
    flex: 1;
    min-width: 0;
    padding: 0.3125rem 0.5rem;
    border: 1px solid var(--input-border);
    border-radius: 0.375rem;
    font-size: 0.8125rem;
  }

  .inline-code {
    font-family: ui-monospace, monospace;
    font-size: 0.75rem;
    background: var(--content-bg);
    padding: 0.1rem 0.3rem;
    border-radius: 0.25rem;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
  }

  .connector-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 1rem;
  }

  .connector-header h3 {
    font-size: 1rem;
    margin: 0 0 0.25rem;
  }

  .note {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0;
  }

  .middlecontrol-sdk-link {
    margin-top: 0.5rem;
  }

  .middlecontrol-devices {
    margin-top: 0.75rem;
    padding: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: var(--content-bg);
  }

  .middlecontrol-devices-head,
  .middlecontrol-devices li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .middlecontrol-devices-head h4 {
    margin: 0;
    font-size: 0.875rem;
  }

  .middlecontrol-devices-head > span,
  .middlecontrol-devices small {
    color: var(--text-secondary);
    font-size: 0.75rem;
  }

  .middlecontrol-devices ul {
    list-style: none;
    margin: 0.5rem 0 0;
    padding: 0;
  }

  .middlecontrol-devices li {
    min-height: 2.5rem;
    border-top: 1px solid var(--border);
  }

  .middlecontrol-devices li strong,
  .middlecontrol-devices li small {
    display: block;
  }

  .middlecontrol-badges {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.25rem;
  }

  .middlecontrol-badges span {
    padding: 0.15rem 0.35rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 0.6875rem;
    color: var(--text-secondary);
  }

  .middlecontrol-badges .recording {
    border-color: var(--status-err-dot);
    color: var(--status-err-text);
  }

  .middlecontrol-badges .ptz {
    border-color: var(--accent);
    color: var(--accent);
  }

  .form-row {
    margin-bottom: 0.75rem;
  }

  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .field--full {
    grid-column: 1 / -1;
  }

  .field label,
  .checkbox-label {
    font-size: 0.875rem;
    color: var(--text-primary);
    font-weight: 500;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  input[type='text'],
  input[type='number'],
  input[type='password'] {
    padding: 0.375rem 0.625rem;
    border: 1px solid var(--input-border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    width: 100%;
    box-sizing: border-box;
    background: var(--input-bg);
  }

  input[type='text']:focus,
  input[type='number']:focus,
  input[type='password']:focus {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-color: var(--accent);
  }

  fieldset {
    border: none;
    padding: 0;
    margin: 0;
  }

  fieldset:disabled input,
  fieldset:disabled label {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .coming-soon-notice {
    font-size: 0.875rem;
    color: var(--text-secondary);
    background: var(--content-bg);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    padding: 0.5rem 0.75rem;
    margin-bottom: 0.75rem;
  }

  .button-row {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .discovery-actions {
    margin: 0.75rem 0;
  }

  .error {
    color: var(--status-err-text);
    font-size: 0.875rem;
    margin: 0 0 0.75rem;
  }

  .btn-primary {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-secondary {
    padding: 0.5rem 1rem;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--accent-subtle);
  }

  .btn-danger {
    padding: 0.5rem 1rem;
    background: var(--status-err-dot);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-danger:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .device-monitor-link {
    display: inline-block;
    margin-top: 1rem;
    font-size: 0.8125rem;
    color: var(--accent);
    text-decoration: none;
  }

  .device-monitor-link:hover {
    text-decoration: underline;
  }
</style>
