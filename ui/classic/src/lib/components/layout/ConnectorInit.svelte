<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    getAppMode,
    getToken,
    getServerPort,
    getLocalHost,
    getClientUrl,
    getClientToken,
    listenToHost,
  } from '@metocast/core-client';
  import { appMode } from '$lib/stores/mode.js';
  import {
    serverUrl,
    serverPort,
    authToken,
    localNetworkUrl,
    appReady,
  } from '$lib/stores/server-url.js';
  import { connectWs, disconnectWs } from '$lib/ws-bindings.js';
  import type { AppMode } from '$lib/stores/mode.js';
  import {
    obsStatus,
    obsConfig,
    obsState,
    vmixStatus,
    vmixConfig,
    atemStatus,
    atemConfig,
    broadlinkStatus,
    broadlinkConfig,
    youtubeStatus,
    youtubeConfig,
    youtubeState,
    facebookStatus,
    facebookConfig,
    facebookState,
    discordStatus,
    discordConfig,
    middlecontrolStatus,
    middlecontrolConfig,
    szentirasConfig,
    youtubeLiveActive,
    mapConnectorStatus,
    applyConnectorStatuses,
  } from '$lib/stores/connectors.js';
  import type { ConnectorStatus } from '$lib/stores/connectors.js';
  import type { Writable } from 'svelte/store';
  import type { ConnectorConfigMap, ConnectorName } from '@metocast/core-client/schemas/connectors';
  import { fetchConnectorConfig, fetchConnectorStatuses } from '@metocast/core-client';
  import { loadSavedLocale } from '$lib/i18n';
  import { findConnector } from '$lib/connectors/registry.js';
  import { pushError, clearErrors } from '$lib/stores/errors.js';

  let unlistenObs: (() => void) | undefined;
  let unlistenYt: (() => void) | undefined;
  let unlistenFb: (() => void) | undefined;

  function syncErrorStore(connectorId: string, status: ConnectorStatus) {
    const def = findConnector(connectorId);
    if (status === 'error') {
      const entry: Parameters<typeof pushError>[0] = {
        connectorId,
        connectorName: def?.name ?? connectorId,
        message: `${def?.name ?? connectorId} connection error`,
      };
      if (def?.infoMarkdown !== undefined) {
        entry.infoMarkdown = def.infoMarkdown;
      }
      pushError(entry);
    } else {
      clearErrors(connectorId);
    }
  }

  $effect(() => {
    syncErrorStore('obs', $obsStatus);
  });
  $effect(() => {
    syncErrorStore('vmix', $vmixStatus);
  });
  $effect(() => {
    syncErrorStore('atem', $atemStatus);
  });
  $effect(() => {
    syncErrorStore('broadlink', $broadlinkStatus);
  });
  $effect(() => {
    syncErrorStore('youtube', $youtubeStatus);
  });
  $effect(() => {
    syncErrorStore('facebook', $facebookStatus);
  });
  $effect(() => {
    syncErrorStore('discord', $discordStatus);
  });
  $effect(() => {
    syncErrorStore('middlecontrol', $middlecontrolStatus);
  });

  $effect(() => {
    youtubeState.update((s) => ({ ...s, isLive: $youtubeLiveActive }));
  });

  onMount(async () => {
    try {
      const mode = await getAppMode();

      if (mode === null) {
        await goto('/setup');
        return;
      }

      appMode.set(mode as AppMode);

      if (mode === 'server') {
        const [token, port, localHost] = await Promise.all([
          getToken(),
          getServerPort(),
          getLocalHost(),
        ]);
        authToken.set(token);
        serverPort.set(port);
        serverUrl.set(`http://localhost:${port}`);
        if (localHost) {
          localNetworkUrl.set(`http://${localHost}:${port}`);
        }
      } else if (mode === 'client') {
        const [url, token] = await Promise.all([getClientUrl(), getClientToken()]);
        if (url) {
          serverUrl.set(url);
        }
        authToken.set(token);
      }
      appReady.set(true);
    } catch (e) {
      console.error('Layout init error:', e);
    }

    loadSavedLocale();

    const currentMode = await getAppMode().catch(() => null);

    if (currentMode === 'server' || currentMode === 'client') {
      try {
        applyConnectorStatuses(await fetchConnectorStatuses());
      } catch (e) {
        console.error('Connector status fetch error:', e);
      }

      async function loadConfig<K extends ConnectorName>(
        name: K,
        store: Writable<ConnectorConfigMap[K]>,
      ) {
        try {
          store.set(await fetchConnectorConfig(name));
        } catch (e) {
          console.error(`${name} config load error:`, e);
        }
      }

      await Promise.all([
        loadConfig('obs', obsConfig),
        loadConfig('vmix', vmixConfig),
        loadConfig('atem', atemConfig),
        loadConfig('broadlink', broadlinkConfig),
        loadConfig('youtube', youtubeConfig),
        loadConfig('facebook', facebookConfig),
        loadConfig('discord', discordConfig),
        loadConfig('middlecontrol', middlecontrolConfig),
        loadConfig('szentiras', szentirasConfig),
      ]);
    }

    // Desktop fast-path: live status pushes from the local core over Tauri events.
    if (currentMode === 'server') {
      unlistenObs = await listenToHost<{ type: string }>('connector://obs-status', (payload) => {
        const mapped = mapConnectorStatus(payload as Parameters<typeof mapConnectorStatus>[0]);
        obsStatus.set(mapped);
        obsState.update((s) => ({ ...s, connection: mapped }));
      });
      unlistenYt = await listenToHost<{ type: string }>('connector://youtube-status', (payload) => {
        const mapped = mapConnectorStatus(payload as Parameters<typeof mapConnectorStatus>[0]);
        youtubeStatus.set(mapped);
        youtubeState.update((s) => ({ ...s, connection: mapped }));
      });
      unlistenFb = await listenToHost<{ type: string }>(
        'connector://facebook-status',
        (payload) => {
          const mapped = mapConnectorStatus(payload as Parameters<typeof mapConnectorStatus>[0]);
          facebookStatus.set(mapped);
          facebookState.update((s) => ({ ...s, connection: mapped }));
        },
      );
    }

    connectWs();
  });

  onDestroy(() => {
    unlistenObs?.();
    unlistenYt?.();
    unlistenFb?.();
    disconnectWs();
  });
</script>
