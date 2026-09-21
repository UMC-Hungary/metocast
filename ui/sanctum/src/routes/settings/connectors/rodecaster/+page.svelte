<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    PageHeader,
    SectionLabel,
    List,
    Badge,
    Dot,
    Glyph,
    Toggle,
    OverviewCell,
    ProgressBar,
    ErrorState,
  } from '@metocast/design-system';
  import { requestRodecasterProfile, setRodecasterMute } from '@metocast/core-client';
  import { live } from '$lib/live.svelte';
  import { pushToast } from '$lib/notifications.svelte';
  import NotifBell from '$lib/NotifBell.svelte';

  let opened = $state<number | null>(null);

  const connected = $derived(live.connectorStatus.rodecaster === 'connected');
  const profile = $derived(live.rodecasterProfile);
  const channels = $derived(profile?.channels ?? []);
  const mutedCount = $derived(channels.filter((c) => c.mute).length);

  onMount(requestRodecasterProfile);

  function toggleMute(channel: number, mute: boolean) {
    if (setRodecasterMute(channel, mute)) return;
    pushToast({
      kind: $_('toast.connector'),
      source: 'RØDECaster Pro II',
      title: $_('rodecaster.toast.muteFail'),
      body: $_('rodecaster.notConnected'),
      tone: 'error',
    });
  }
</script>

<PageHeader
  title={$_('rodecaster.title')}
  back={{ label: $_('rodecaster.back'), href: '/settings/connectors' }}
>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

{#if !connected || !profile}
  <ErrorState
    title={$_('rodecaster.unavailable')}
    body={$_('rodecaster.notConnected')}
    retryLabel={$_('rodecaster.retry')}
    onretry={requestRodecasterProfile}
  />
{:else}
  <section class="overview">
    <OverviewCell
      label={$_('rodecaster.overview.device')}
      value={profile.model}
      color="var(--status-ok)"
    />
    <OverviewCell
      label={$_('rodecaster.overview.channels')}
      value={channels.length}
      color="var(--status-ok)"
      divider
    />
    <OverviewCell
      label={$_('rodecaster.overview.muted')}
      value={mutedCount}
      color={mutedCount ? 'var(--status-warn)' : 'var(--status-off)'}
      divider
    />
  </section>

  <SectionLabel hint={$_('rodecaster.readOnlyHint')}>{$_('rodecaster.channels')}</SectionLabel>
  <List>
    {#each channels as channel (channel.channel)}
      <div class="channel" class:muted={channel.mute}>
        <button
          class="head"
          type="button"
          aria-expanded={opened === channel.channel}
          onclick={() => (opened = opened === channel.channel ? null : channel.channel)}
        >
          <Glyph char={channel.mute ? '⨯' : '⇵'} size={34} />
          <span class="text">
            <strong>{channel.label}</strong>
            <em>
              <Dot color={channel.mute ? 'var(--status-warn)' : 'var(--status-ok)'} size={5} />
              {$_('rodecaster.channelMeta', {
                values: {
                  level: Math.round(channel.level * 100),
                  blocks: channel.processing.length,
                },
              })}
            </em>
            {#if channel.wirelessMute}
              <span class="remote"
                ><Badge tone="warn" dot>{$_('rodecaster.remoteMicMuted')}</Badge></span
              >
            {/if}
          </span>
        </button>
        <Toggle
          checked={!channel.mute}
          label={channel.label}
          onchange={() => toggleMute(channel.channel, !channel.mute)}
        />
      </div>

      {#if opened === channel.channel}
        <section class="detail">
          <ProgressBar
            label={$_('rodecaster.level')}
            value={Math.round(channel.level * 100)}
            valueText={`${Math.round(channel.level * 100)}%`}
          />
          {#if channel.cue}<p>{$_('rodecaster.cue')}</p>{/if}
          {#if channel.processing.length}
            <div class="chips">
              {#each channel.processing as block (block)}
                <Badge tone="ok" dot>{block}</Badge>
              {/each}
            </div>
          {/if}
          {#if channel.settings.length}
            <dl>
              {#each channel.settings as field (field.name)}
                <div>
                  <dt>{field.name}</dt>
                  <dd>{field.value}</dd>
                </div>
              {/each}
            </dl>
          {:else}
            <p>{$_('rodecaster.noSettings')}</p>
          {/if}
        </section>
      {/if}
    {/each}
  </List>

  <p class="note footer">{$_('rodecaster.note')}</p>
{/if}

<style>
  .overview {
    margin: 0 24px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border-block: 1px solid var(--border-hairline);
  }
  .channel {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 24px;
    border-bottom: 1px solid var(--border-hairline);
  }
  .muted {
    opacity: 0.72;
  }
  .head {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 14px;
    background: transparent;
    border: 0;
    padding: 0;
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
    font-family: inherit;
  }
  .text {
    min-width: 0;
  }
  strong {
    display: block;
    font-size: 15px;
    font-weight: 500;
    color: var(--text-primary);
  }
  em {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-secondary);
    font-style: normal;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .remote {
    display: block;
    margin-top: 6px;
  }
  .detail {
    padding: 14px 24px 18px 72px;
    border-bottom: 1px solid var(--border-hairline);
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
  }
  .detail p {
    margin: 0;
    font-family: var(--font-display);
    font-style: italic;
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1.45;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  dl {
    margin: 0;
    display: grid;
    gap: 6px;
  }
  dl div {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    font-size: 13px;
  }
  dt {
    color: var(--text-muted);
    min-width: 0;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .note {
    margin: 0;
    font-family: var(--font-display);
    font-style: italic;
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1.45;
  }
  .footer {
    padding: 28px 32px 96px;
    text-align: center;
  }
</style>
