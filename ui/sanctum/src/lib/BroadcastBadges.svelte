<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { Badge } from '@metocast/design-system';
  import type { EventConnection } from '@metocast/core-client/schemas/event';
  import { siFacebook, siYoutube } from './brand-marks';

  let { connections }: { connections: EventConnection[] } = $props();

  const MARKS: Record<string, string> = { youtube: siYoutube.path, facebook: siFacebook.path };
  const NAMES: Record<string, string> = { youtube: 'YouTube', facebook: 'Facebook' };
  const TONE = { scheduled: 'ok', starting: 'warn', live: 'live', ended: 'neutral' } as const;
  type Shown = keyof typeof TONE;
  const isShown = (status: string): status is Shown => status in TONE;

  const shown = $derived(
    connections.flatMap((c) =>
      c.externalId && isShown(c.scheduleStatus)
        ? [{ platform: c.platform, status: c.scheduleStatus }]
        : [],
    ),
  );
</script>

{#if shown.length}
  <span class="badges">
    {#each shown as b (b.platform)}
      {@const state = $_(`events.broadcast.status.${b.status}`)}
      <span title={`${NAMES[b.platform] ?? b.platform}: ${state}`}>
        <Badge tone={TONE[b.status]} dot={b.status === 'live' || b.status === 'starting'}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d={MARKS[b.platform] ?? ''} /></svg>
          <span class="sr">{NAMES[b.platform] ?? b.platform}</span>
          {state}
        </Badge>
      </span>
    {/each}
  </span>
{/if}

<style>
  .badges {
    display: inline-flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  svg {
    width: 11px;
    height: 11px;
    fill: currentColor;
    vertical-align: -1px;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
