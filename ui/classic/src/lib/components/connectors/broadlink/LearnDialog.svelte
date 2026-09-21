<script lang="ts">
  import { onDestroy } from 'svelte';
  import { startLearn, cancelLearn, addCommand, type BroadlinkDevice } from '@metocast/core-client';
  import { broadlinkLearnResult } from '$lib/stores/broadlink.js';

  interface Props {
    device: BroadlinkDevice;
    onDone: () => void;
    onCancel: () => void;
  }

  let { device, onDone, onCancel }: Props = $props();

  let signalType = $state<'ir' | 'rf'>('ir');
  let commandName = $state('');
  let commandSlug = $state('');
  let category = $state('other');
  let learnedCode = $state<string | null>(null);
  let learnError = $state<string | null>(null);
  let learning = $state(false);
  let saving = $state(false);
  let saveError = $state('');

  const CATEGORIES = ['projector', 'screen', 'hvac', 'lighting', 'audio', 'other'] as const;

  const unsubscribe = broadlinkLearnResult.subscribe((result) => {
    if (!result || !learning) return;
    learning = false;
    learnedCode = result.code;
    learnError = result.error;
  });

  onDestroy(() => {
    unsubscribe();
  });

  async function handleStart() {
    learnedCode = null;
    learnError = null;
    learning = true;
    broadlinkLearnResult.set(null);
    try {
      await startLearn(device.id, signalType);
    } catch (e) {
      learning = false;
      learnError = String(e);
    }
  }

  async function handleCancel() {
    if (learning) {
      await cancelLearn();
      learning = false;
    }
    onCancel();
  }

  async function handleSave() {
    if (!learnedCode) return;
    saving = true;
    saveError = '';
    try {
      await addCommand({
        deviceId: device.id,
        name: commandName,
        slug: commandSlug || commandName.toLowerCase().replace(/\s+/g, '-'),
        code: learnedCode,
        codeType: signalType,
        category,
      });
      onDone();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<div
  role="dialog"
  aria-modal="true"
  aria-label="Learn IR/RF Code"
  style="position:fixed;inset:0;background:var(--modal-backdrop);display:flex;align-items:center;justify-content:center;z-index:100;"
>
  <div
    style="background:var(--modal-card-bg);padding:1.5rem;border-radius:0.5rem;min-width:340px;max-width:90vw;"
  >
    <h2>Learn IR/RF Code — {device.name}</h2>

    {#if !learnedCode}
      <div style="margin-bottom:0.75rem;">
        <label>
          Signal type:
          <select bind:value={signalType}>
            <option value="ir">IR</option>
            <option value="rf">RF</option>
          </select>
        </label>
      </div>

      {#if learning}
        <p>Point your remote at the device and press the button…</p>
      {/if}

      {#if learnError}
        <p style="color:var(--status-err-text)">{learnError}</p>
      {/if}

      <div style="display:flex;gap:0.5rem;margin-top:1rem;">
        {#if !learning}
          <button onclick={handleStart}>Start Learning</button>
        {/if}
        <button onclick={handleCancel}>{learning ? 'Cancel' : 'Close'}</button>
      </div>
    {:else}
      <p style="color:var(--status-ok-text)">Code captured! ({learnedCode.length / 2} bytes)</p>

      <div style="display:flex;flex-direction:column;gap:0.5rem;margin-top:0.75rem;">
        <input placeholder="Command name" bind:value={commandName} />
        <input placeholder="Slug (auto-generated if empty)" bind:value={commandSlug} />
        <label>
          Category:
          <select bind:value={category}>
            {#each CATEGORIES as cat (cat)}
              <option value={cat}>{cat}</option>
            {/each}
          </select>
        </label>
        {#if saveError}
          <span style="color:var(--status-err-text)">{saveError}</span>
        {/if}
      </div>

      <div style="display:flex;gap:0.5rem;margin-top:1rem;">
        <button onclick={handleSave} disabled={!commandName || saving}>
          {saving ? 'Saving…' : 'Save Command'}
        </button>
        <button onclick={handleCancel}>Cancel</button>
      </div>
    {/if}
  </div>
</div>
