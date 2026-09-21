<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { Segmented, Field, Button, FlameMark } from '@metocast/design-system';
  import { canHostServer, completeSetup, parseConnectLink, probeCore } from '@metocast/core-client';
  import type { ConnectLink, HostMode } from '@metocast/core-client';
  import jsQR from 'jsqr';

  let { connect = null }: { connect?: ConnectLink | null } = $props();

  let canServe = $state(true);
  let mode = $state<HostMode>('server');

  onMount(async () => {
    try {
      canServe = await canHostServer();
    } catch {
      /* no host — leave the desktop default */
    }
    if (!canServe) mode = 'client';
  });
  let clientUrl = $state('');
  let clientToken = $state('');
  let error = $state('');
  let busy = $state(false);
  let scanning = $state(false);
  let scanError = $state('');
  let video = $state<HTMLVideoElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let stream: MediaStream | null = null;

  async function startScan() {
    scanError = '';
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' } });
      scanning = true;
      await tick();
      if (!video) throw new Error('Video element not ready');
      video.srcObject = stream;
      await video.play();
      requestAnimationFrame(scanFrame);
    } catch {
      scanError = $_('setup.scanError');
      stream?.getTracks().forEach((t) => t.stop());
      stream = null;
    }
  }

  function scanFrame() {
    if (!scanning || !video || !canvas) return;
    if (video.readyState >= video.HAVE_ENOUGH_DATA) {
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.drawImage(video, 0, 0);
      const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const code = jsQR(img.data, img.width, img.height);
      const link = code && parseConnectLink(code.data);
      if (link) {
        stopScan();
        apply(link);
        return;
      }
    }
    requestAnimationFrame(scanFrame);
  }

  function stopScan() {
    scanning = false;
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
  }

  function apply(link: ConnectLink) {
    mode = 'client';
    clientUrl = link.url;
    clientToken = link.token;
    confirm();
  }

  $effect(() => {
    if (connect) apply(connect);
  });

  const modeOptions = $derived([
    { value: 'server', label: $_('setup.server') },
    { value: 'client', label: $_('setup.client') },
  ]);

  async function confirm(): Promise<void> {
    error = '';
    busy = true;
    try {
      if (mode === 'client') {
        const url = clientUrl.trim().replace(/\/$/, '');
        const token = clientToken.trim();
        if (!url || !token) {
          error = $_('setup.missing');
          return;
        }
        const probe = await probeCore(url, token);
        if (!probe.ok) {
          error =
            probe.reason === 'unreachable'
              ? $_('setup.unreachable')
              : probe.reason === 'unauthorized'
                ? $_('setup.unauthorized')
                : $_('setup.unexpected', { values: { status: probe.status ?? '' } });
          return;
        }
        await completeSetup({ mode: 'client', serverUrl: url, clientToken: token });
      } else {
        await completeSetup({ mode: 'server' });
      }
      window.location.href = '/';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="setup">
  <div class="card">
    <header class="brand">
      <FlameMark size={26} />
      <span>Metocast</span>
    </header>
    <p class="eyebrow">{$_('setup.eyebrow')}</p>
    <h1>{$_('setup.title')}</h1>
    <p class="lede">{$_('setup.lede')}</p>

    <form
      onsubmit={(e) => {
        e.preventDefault();
        confirm();
      }}
    >
      {#if canServe}
        <Segmented
          label={$_('setup.modeLabel')}
          value={mode}
          options={modeOptions}
          onchange={(v) => (mode = v === 'client' ? 'client' : 'server')}
        />
      {/if}
      <p class="hint">{mode === 'server' ? $_('setup.serverHint') : $_('setup.clientHint')}</p>

      {#if mode === 'client'}
        <Button type="button" variant="secondary" block onclick={startScan}>
          {$_('setup.scanQr')}
        </Button>
        {#if scanError}<p class="error" role="alert">{scanError}</p>{/if}
        <div class="fields">
          <Field
            label={$_('setup.urlLabel')}
            bind:value={clientUrl}
            type="url"
            placeholder="https://example.com"
          />
          <Field
            label={$_('setup.tokenLabel')}
            bind:value={clientToken}
            placeholder={$_('setup.tokenPlaceholder')}
          />
        </div>
      {/if}

      {#if error}<p class="error" role="alert">{error}</p>{/if}

      <Button type="submit" variant="primary" block disabled={busy}>
        {busy ? $_('setup.working') : $_('setup.confirm')}
      </Button>
    </form>
  </div>
</div>

{#if scanning}
  <div class="scan-overlay">
    <p class="scan-hint">{$_('setup.scanning')}</p>
    <video bind:this={video} playsinline muted class="scan-video"></video>
    <canvas bind:this={canvas} hidden></canvas>
    <button type="button" class="scan-cancel" onclick={stopScan}>{$_('setup.scanCancel')}</button>
  </div>
{/if}

<style>
  .setup {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    overflow-y: auto;
    padding: max(24px, env(safe-area-inset-top, 24px)) 24px
      max(24px, env(safe-area-inset-bottom, 24px));
    box-sizing: border-box;
  }
  .card {
    width: 100%;
    max-width: 420px;
    background: var(--surface-raised);
    border: 1px solid color-mix(in srgb, var(--text-primary) 12%, transparent);
    padding: 30px 28px 28px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 22px;
  }
  .brand span {
    font-family: var(--font-display);
    font-style: italic;
    font-size: 17px;
    font-weight: 500;
    color: var(--text-primary);
  }
  .eyebrow {
    margin: 0;
    font-family: var(--font-label);
    font-size: 10px;
    letter-spacing: 1.6px;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  h1 {
    margin: 6px 0 8px;
    font-family: var(--font-display);
    font-size: 26px;
    font-weight: 500;
    color: var(--text-primary);
  }
  .lede {
    margin: 0 0 22px;
    font-size: 14px;
    color: var(--text-secondary, var(--text-muted));
    line-height: 1.5;
  }
  form {
    display: grid;
    gap: 14px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.45;
  }
  .fields {
    display: grid;
    gap: 12px;
  }
  .error {
    margin: 0;
    font-size: 13px;
    color: var(--status-err-text, var(--status-warn));
  }
  .scan-overlay {
    position: fixed;
    inset: 0;
    background: #000;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: space-between;
    padding: max(24px, env(safe-area-inset-top, 24px)) 24px
      max(24px, env(safe-area-inset-bottom, 24px));
    box-sizing: border-box;
    z-index: 200;
  }
  .scan-hint {
    color: #fff;
    font-size: 14px;
    text-align: center;
    margin: 0;
    line-height: 1.4;
  }
  .scan-video {
    width: 100%;
    max-width: 380px;
    aspect-ratio: 1;
    object-fit: cover;
  }
  .scan-cancel {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.4);
    color: #fff;
    font-family: var(--font-label);
    font-size: 12px;
    letter-spacing: 1.4px;
    text-transform: uppercase;
    padding: 12px 32px;
    cursor: pointer;
  }
</style>
