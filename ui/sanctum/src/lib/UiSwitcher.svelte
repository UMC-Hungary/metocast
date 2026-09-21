<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { SectionLabel, List, Row, TextIcon } from '@metocast/design-system';
  import { loadBundledUis, getActiveUi, setActiveUi } from '@metocast/core-client';
  import type { BundledUi } from '@metocast/core-client';

  let uis = $state<BundledUi[]>([]);
  let active = $state('');

  onMount(async () => {
    const manifest = await loadBundledUis();
    if (!manifest) return;
    uis = manifest.uis;
    active = getActiveUi(manifest.active);
  });

  // Persist over the shared key, then load the chosen UI immediately.
  function choose(ui: BundledUi) {
    setActiveUi(ui.id);
    window.location.assign(ui.path);
  }
</script>

{#if uis.length > 1}
  <SectionLabel>{$_('ui.activeInterface')}</SectionLabel>
  <List>
    {#each uis as ui, i (ui.id)}
      <Row
        title={ui.displayName}
        meta={ui.description}
        detail={active === ui.id ? $_('ui.current') : undefined}
        onclick={() => choose(ui)}
        last={i === uis.length - 1}
      >
        {#snippet icon()}<TextIcon char={active === ui.id ? '●' : '○'} />{/snippet}
      </Row>
    {/each}
  </List>
{/if}
