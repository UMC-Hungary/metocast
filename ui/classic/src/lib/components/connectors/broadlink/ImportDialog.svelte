<script lang="ts">
  import { addCommand, type BroadlinkDevice } from '@metocast/core-client';

  interface Props {
    device: BroadlinkDevice;
    onDone: () => void;
    onCancel: () => void;
  }

  let { device, onDone, onCancel }: Props = $props();

  type PresetCmd = {
    name: string;
    filename: string;
    category: string;
    codeType: 'ir' | 'rf';
  };

  const PRESETS: PresetCmd[] = [
    { name: 'Projector On', filename: 'NEC.on', category: 'projector', codeType: 'ir' },
    { name: 'Projector Off', filename: 'NEC.off', category: 'projector', codeType: 'ir' },
    { name: 'Projector Menu', filename: 'NEC.menu', category: 'projector', codeType: 'ir' },
    { name: 'Projector OK', filename: 'NEC.ok', category: 'projector', codeType: 'ir' },
    { name: 'Projector Up', filename: 'NEC.up', category: 'projector', codeType: 'ir' },
    { name: 'Projector Back', filename: 'NEC.back', category: 'projector', codeType: 'ir' },
    { name: 'Projector Left', filename: 'NEC.left', category: 'projector', codeType: 'ir' },
    { name: 'Projector Right', filename: 'NEC.right', category: 'projector', codeType: 'ir' },
    { name: 'Projector Power', filename: 'NEC.power', category: 'projector', codeType: 'ir' },
    { name: 'Picture Mute', filename: 'NEC.picture_mute', category: 'projector', codeType: 'ir' },
    { name: 'Screen Up', filename: 'ROLL.up', category: 'screen', codeType: 'rf' },
    { name: 'Screen Down', filename: 'ROLL.down', category: 'screen', codeType: 'rf' },
    { name: 'Screen Stop', filename: 'ROLL.stop', category: 'screen', codeType: 'rf' },
    { name: 'AC Temp Up', filename: 'CASCADE.temp_up', category: 'hvac', codeType: 'ir' },
  ];

  const CODES: Record<string, string> = {
    'NEC.on':
      '2600500000012396121312131213123713371114121311141237121312131238121311381238113811141213111412371213121312131313123712371237121312381237123712381200054b0001264b11000d05',
    'NEC.off':
      '2600500000012395121411131114123712381114111411141238111412131238121311381238113811141213111412131238111412381237123812371237123712381200054c0001254b12000d05',
    'NEC.menu':
      '2600500000012396121312141213123712381113121311141237121312131238121312371238123711141213111412131238121311141237123812371237123712381200054c0001254b12000d05',
    'NEC.ok':
      '2600500000012395121411131114123712381113121311141237121312131238121312371238113812131114111412131238121311141114123812371237123712381200054b0001264b11000d05',
    'NEC.up':
      '2600500000012396121312131213123712381114111411141237121312131238121311381238113811141213121311141238111412381237121312381237123712381200054b0001264b11000d05',
    'NEC.back':
      '2600500000012396111412131213123712381113121312131238111412131238121312371238113811141213121311141238111412371238121312381237123712381200054b0001264a12000d05',
    'NEC.left':
      '2600500000012396121312131213123712381114111411141237121411131238121312371238113811141213111412371114123811141237123812371237123712371200054c0001254b12000d05',
    'NEC.right':
      '2600500000012395121411131114123712381113121312131238111412131238121311381237123712141114111412131238121311141114123812371237123712381200054b0001264b11000d05',
    'NEC.power':
      '2600500000012396121312131213123712381114111412131238121312131238121311381238113811141213111412381213111412381237123712381237123712381200054b0001264b11000d05',
    'NEC.picture_mute':
      '2600500000012396121312131213123712381114111411141238121312131238121312371238113811141213111412371213121311141238123712381237123712381200054b0001264b11000d05',
    'ROLL.up':
      'b1c0fc01469f060008bc07111405140507111505130513060712130513070612061312070612130612071306120607130513051307121306051307bd07121207130605131207130612060713120612070613051312070613120612081107120705140514041406131207051306be06121307120606121307120612070613120712060613061312060613120712061208120605140613051305131208051305c005131207110805131207120712060614120612070613051312070613120612071208110705130613051405131208041405bf05131207120705131207120712070513120811070514061312060613120712061208110705140514051305131208051305c005131206120805131206120812060514120712070513061312070513120712071107120805130515041405131208041405bf05131207120705141107120811070513120812060513071312060514110812071107120804140513061305141107061305bf06131207110706131207120712060613120712070513061312070513120811071207110805130514061305131207061305be07131206120706131206120712070514110712080414051411080414120711081107120805130513061405131206061306be061312071206071213061206120805131207110805130613110805131207120712071107061404140514051411070514050005dc',
    'ROLL.down':
      'b1c0fc018d9e05000a3e061312071207060b131007130612070613060712070514051405140514050712061311080612051306be0612130712060613110812060713061206140506140614051406140505130614061306150504140505140605140505bd0613120712060613110812060613070612061305071305140515041405051306140614051504051405051306051405050005dc',
    'ROLL.stop':
      'b1c0fc014e9f0500094906131307110706131307120712060614110712070514051312070514120612071208110705131406130605140514060613050005dc',
    'CASCADE.temp_up':
      '2600a600010195161015101510161015101510151016100f1610151010151010151016100f16100f16101510151010151016100f1610151010151015101510151016100f16101510101510161015100f16101510151010151015101510151010151010151016100f1610151016100f16100f1610151010151016100f161010151015100f160001a00100960f16100f16101510151015101510151015100005dc',
  };

  function toSlug(name: string, existing: string[]): string {
    let base = name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '');
    let slug = base;
    let i = 2;
    while (existing.includes(slug)) slug = `${base}-${i++}`;
    return slug;
  }

  let selected = $state<Set<string>>(new Set(PRESETS.map((p) => p.filename)));
  let importing = $state(false);
  let error = $state('');
  let result = $state<{ imported: number; skipped: number } | null>(null);

  function toggle(filename: string) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- copy-then-assign; reactivity comes from the $state assignment
    const next = new Set(selected);
    if (next.has(filename)) next.delete(filename);
    else next.add(filename);
    selected = next;
  }

  async function handleImport() {
    importing = true;
    error = '';
    result = null;

    const usedSlugs: string[] = [];
    let imported = 0;
    let skipped = 0;

    for (const preset of PRESETS) {
      if (!selected.has(preset.filename)) continue;
      const code = CODES[preset.filename];
      if (!code) {
        skipped++;
        continue;
      }

      const slug = toSlug(preset.name, usedSlugs);
      usedSlugs.push(slug);

      try {
        await addCommand({
          deviceId: device.id,
          name: preset.name,
          slug,
          code,
          codeType: preset.codeType,
          category: preset.category,
        });
        imported++;
      } catch {
        skipped++;
      }
    }

    importing = false;
    result = { imported, skipped };
  }

  function handleDone() {
    result = null;
    onDone();
  }
</script>

<div
  class="backdrop"
  role="presentation"
  onclick={onCancel}
  onkeydown={(e) => e.key === 'Escape' && onCancel()}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Import predefined commands"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div class="dialog-header">
      <h2 class="dialog-title">Import Predefined Commands</h2>
      <p class="dialog-subtitle">
        Select commands to import for <strong>{device.name}</strong>. These are pre-learned IR/RF
        codes from metocast-service.
      </p>
    </div>

    {#if result}
      <div class="result-box">
        <p class="result-text">
          Imported <strong>{result.imported}</strong> command{result.imported !== 1
            ? 's'
            : ''}{result.skipped > 0 ? `, skipped ${result.skipped}` : ''}.
        </p>
      </div>
      <div class="dialog-footer">
        <button class="btn-primary" onclick={handleDone}>Done</button>
      </div>
    {:else}
      <div class="controls-row">
        <span class="selected-count">{selected.size} / {PRESETS.length} selected</span>
        <div class="select-links">
          <button
            class="link-btn"
            onclick={() => (selected = new Set(PRESETS.map((p) => p.filename)))}>All</button
          >
          <span>·</span>
          <button class="link-btn" onclick={() => (selected = new Set())}>None</button>
        </div>
      </div>

      <ul class="preset-list" aria-label="Commands to import">
        {#each PRESETS as preset (preset.filename)}
          {@const hasCode = !!CODES[preset.filename]}
          <li class="preset-item" class:preset-item--disabled={!hasCode}>
            <label class="preset-label">
              <input
                type="checkbox"
                checked={selected.has(preset.filename)}
                disabled={!hasCode}
                onchange={() => toggle(preset.filename)}
              />
              <span class="preset-name">{preset.name}</span>
              <span class="preset-meta">
                <span class="badge badge--{preset.codeType}">{preset.codeType.toUpperCase()}</span>
                <span class="preset-cat">{preset.category}</span>
              </span>
            </label>
          </li>
        {/each}
      </ul>

      {#if error}
        <p class="error-text" role="alert">{error}</p>
      {/if}

      <div class="dialog-footer">
        <button class="btn-secondary" onclick={onCancel} disabled={importing}>Cancel</button>
        <button
          class="btn-primary"
          onclick={handleImport}
          disabled={importing || selected.size === 0}
        >
          {importing
            ? 'Importing…'
            : `Import ${selected.size} command${selected.size !== 1 ? 's' : ''}`}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--modal-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .dialog {
    background: var(--modal-card-bg);
    border-radius: 0.5rem;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
    width: min(520px, calc(100vw - 2rem));
    max-height: min(80vh, 640px);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .dialog-header {
    padding: 1.25rem 1.25rem 0;
    flex-shrink: 0;
  }

  .dialog-title {
    font-size: 1rem;
    font-weight: 600;
    margin: 0 0 0.375rem;
  }

  .dialog-subtitle {
    font-size: 0.8125rem;
    color: var(--text-secondary);
    margin: 0 0 1rem;
  }

  .controls-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.25rem 0.5rem;
    flex-shrink: 0;
  }

  .selected-count {
    font-size: 0.8125rem;
    color: var(--text-secondary);
  }

  .select-links {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.8125rem;
    color: var(--text-tertiary);
  }

  .link-btn {
    background: none;
    border: none;
    padding: 0;
    font-size: 0.8125rem;
    color: var(--accent);
    cursor: pointer;
  }

  .link-btn:hover {
    text-decoration: underline;
  }

  .preset-list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }

  .preset-item {
    border-bottom: 1px solid var(--border);
  }

  .preset-item:last-child {
    border-bottom: none;
  }

  .preset-item--disabled {
    opacity: 0.45;
  }

  .preset-label {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    padding: 0.6rem 1.25rem;
    cursor: pointer;
  }

  .preset-item--disabled .preset-label {
    cursor: not-allowed;
  }

  .preset-name {
    flex: 1;
    font-size: 0.875rem;
    font-weight: 500;
  }

  .preset-meta {
    display: flex;
    align-items: center;
    gap: 0.375rem;
  }

  .badge {
    font-size: 0.6875rem;
    font-weight: 600;
    padding: 0.1rem 0.4rem;
    border-radius: 99px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .badge--ir {
    background: var(--accent-subtle);
    color: var(--accent);
  }

  .badge--rf {
    background: var(--status-warn-bg);
    color: var(--status-warn-text);
  }

  .preset-cat {
    font-size: 0.75rem;
    color: var(--text-tertiary);
    text-transform: capitalize;
  }

  .result-box {
    padding: 1rem 1.25rem;
    background: var(--status-ok-bg);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }

  .result-text {
    font-size: 0.875rem;
    color: var(--status-ok-text);
    margin: 0;
  }

  .error-text {
    font-size: 0.8125rem;
    color: var(--status-err-text);
    padding: 0 1.25rem;
    margin: 0.5rem 0 0;
  }

  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    padding: 1rem 1.25rem;
    flex-shrink: 0;
  }

  .btn-primary {
    padding: 0.4rem 1rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-secondary {
    padding: 0.4rem 1rem;
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--content-bg);
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
