<script lang="ts">
  import { addCommand, type BroadlinkDevice } from '@metocast/core-client';

  interface Props {
    device: BroadlinkDevice;
    onDone: () => void;
    onCancel: () => void;
  }

  let { device, onDone, onCancel }: Props = $props();

  let commandName = $state('');
  let commandSlug = $state('');
  let code = $state('');
  let codeType = $state<'ir' | 'rf'>('ir');
  let category = $state('other');
  let saving = $state(false);
  let saveError = $state('');

  const CATEGORIES = ['projector', 'screen', 'hvac', 'lighting', 'audio', 'other'] as const;

  async function handleSave() {
    saving = true;
    saveError = '';
    try {
      await addCommand({
        deviceId: device.id,
        name: commandName,
        slug: commandSlug || commandName.toLowerCase().replace(/\s+/g, '-'),
        code,
        codeType,
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
  aria-label="Enter IR/RF Code"
  style="position:fixed;inset:0;background:var(--modal-backdrop);display:flex;align-items:center;justify-content:center;z-index:100;"
>
  <div
    style="background:var(--modal-card-bg);padding:1.5rem;border-radius:0.5rem;min-width:340px;max-width:90vw;"
  >
    <h2>Enter Code Manually — {device.name}</h2>

    <div style="display:flex;flex-direction:column;gap:0.5rem;">
      <input placeholder="Command name" bind:value={commandName} />
      <input placeholder="Slug (auto-generated if empty)" bind:value={commandSlug} />
      <label>
        Code type:
        <select bind:value={codeType}>
          <option value="ir">IR</option>
          <option value="rf">RF</option>
        </select>
      </label>
      <textarea
        placeholder="Hex code (e.g. 2600...)"
        bind:value={code}
        rows={4}
        style="font-family:monospace;font-size:0.8rem;"
      ></textarea>
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
      <button onclick={handleSave} disabled={!commandName || !code || saving}>
        {saving ? 'Saving…' : 'Save Command'}
      </button>
      <button onclick={onCancel}>Cancel</button>
    </div>
  </div>
</div>
