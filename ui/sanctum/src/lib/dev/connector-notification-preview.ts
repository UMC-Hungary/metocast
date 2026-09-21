import { goto } from '$app/navigation';
import { siBlackmagicdesign } from 'simple-icons';
import { dismissRail, notify, resolveByKey } from '$lib/notifications.svelte';

const CAMERA_KEY = 'preview:connector:blackmagic-camera';
const DESTINATIONS_KEY = 'preview:connector:streaming-destinations';
let seeded = false;

const cameraBase = {
  issue: true,
  kind: 'Camera',
  source: 'Blackmagic Camera',
  title: 'Blackmagic Camera disconnected',
  brand: siBlackmagicdesign.path,
  key: CAMERA_KEY,
  remediation: [
    'Check that the camera is powered on and connected to the same network',
    'Confirm the camera address in Settings → Connectors',
    'Try reconnecting after the camera becomes reachable',
  ],
};

const editCamera = {
  label: 'Edit',
  run: () => void goto('/settings/connectors?open=blackmagic-camera'),
};

async function reconnectCamera(): Promise<void> {
  notify({
    ...cameraBase,
    tier: 'warn',
    state: 'reconnecting',
    body: 'Looking for cinema-camera-6k.local…',
    actions: [editCamera],
  });
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 1400);
  });
  resolveByKey(CAMERA_KEY);
}

/** Safe, deterministic data for reviewing the complete notification UX in the real app shell. */
export function seedConnectorNotificationPreview(): void {
  if (seeded) return;
  seeded = true;

  const historyId = notify({
    tier: 'ok',
    kind: 'Upload',
    source: 'YouTube',
    title: 'Sunday service uploaded',
    body: 'The recording is ready in YouTube Studio.',
    persistent: false,
  });
  dismissRail(historyId);

  const destinationsId = notify({
    issue: true,
    tier: 'error',
    kind: 'Streaming',
    source: 'YouTube + Facebook',
    title: 'No streaming destination connected',
    body: 'Connect YouTube or Facebook before the next broadcast.',
    state: 'action needed',
    group: [
      { source: 'YouTube', label: 'Not connected' },
      { source: 'Facebook', label: 'Not connected' },
    ],
    actions: [
      {
        label: 'Open connector settings',
        primary: true,
        run: () => void goto('/settings/connectors'),
      },
    ],
    key: DESTINATIONS_KEY,
  });
  dismissRail(destinationsId);

  notify({
    ...cameraBase,
    tier: 'error',
    state: 'error',
    body: 'unreachable: error sending request for url (http://cinema-camera-6k.local/control/api/v1/system/product)',
    actions: [{ label: 'Reconnect', primary: true, run: reconnectCamera }, editCamera],
  });
}
