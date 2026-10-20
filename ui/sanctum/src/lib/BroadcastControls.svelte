<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { Button, Dialog } from '@metocast/design-system';
  import { ApiError, controlEventStream } from '@metocast/core-client';
  import type { BroadcastPlatform, EventConnection } from '@metocast/core-client/schemas/event';
  import { pushToast } from './notifications.svelte';

  let { eventId, connections }: { eventId: string; connections: EventConnection[] } = $props();

  const NAMES: Record<BroadcastPlatform, string> = { youtube: 'YouTube', facebook: 'Facebook' };
  const isPlatform = (p: string): p is BroadcastPlatform => p === 'youtube' || p === 'facebook';

  const actions = $derived(
    connections.flatMap((c) => {
      if (!isPlatform(c.platform) || !c.externalId) return [];
      if (c.scheduleStatus === 'scheduled') return [{ platform: c.platform, start: true }];
      if (c.scheduleStatus === 'starting' || c.scheduleStatus === 'live')
        return [{ platform: c.platform, start: false }];
      return [];
    }),
  );

  let pending = $state<{ platform: BroadcastPlatform; start: boolean } | null>(null);
  let open = $state(false);
  let busy = $state(false);

  function ask(action: { platform: BroadcastPlatform; start: boolean }) {
    pending = action;
    open = true;
  }

  async function confirm() {
    if (!pending) return;
    const { platform, start } = pending;
    const name = NAMES[platform];
    busy = true;
    try {
      await controlEventStream(eventId, platform, start ? 'start' : 'stop');
    } catch (err) {
      const code = err instanceof ApiError ? err.code : null;
      const key = `events.broadcast.errors.${code ?? ''}`;
      const known = code !== null && $_(key) !== key;
      pushToast({
        kind: name,
        source: name,
        title: $_('events.broadcast.failed'),
        body: known ? $_(key, { values: { platform: name } }) : String(code ?? err),
        tone: 'error',
      });
    } finally {
      busy = false;
      open = false;
      pending = null;
    }
  }
</script>

{#each actions as a (a.platform)}
  <Button variant={a.start ? 'primary' : 'danger'} compact onclick={() => ask(a)}>
    {$_(a.start ? 'events.broadcast.goLive' : 'events.broadcast.end', {
      values: { platform: NAMES[a.platform] },
    })}
  </Button>
{/each}

{#if pending}
  {@const name = NAMES[pending.platform]}
  <Dialog
    bind:open
    modalOnly
    title={$_(
      pending.start ? 'events.broadcast.confirmStartTitle' : 'events.broadcast.confirmEndTitle',
      {
        values: { platform: name },
      },
    )}
    onclose={() => (pending = null)}
  >
    <p>
      {$_(pending.start ? 'events.broadcast.confirmStartBody' : 'events.broadcast.confirmEndBody')}
    </p>
    {#snippet footer()}
      <Button variant="quiet" disabled={busy} onclick={() => (open = false)}
        >{$_('events.broadcast.cancel')}</Button
      >
      <Button variant={pending?.start ? 'primary' : 'danger'} disabled={busy} onclick={confirm}>
        {$_(pending?.start ? 'events.broadcast.confirmStart' : 'events.broadcast.confirmEnd')}
      </Button>
    {/snippet}
  </Dialog>
{/if}
