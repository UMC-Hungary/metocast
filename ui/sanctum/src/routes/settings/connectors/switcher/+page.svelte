<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    PageHeader,
    SectionLabel,
    List,
    Row,
    OverviewCell,
    Segmented,
    Spinner,
    TransportDock,
    ErrorState,
  } from '@metocast/design-system';
  import { sendWsCommand } from '@metocast/core-client';
  import { live } from '$lib/live.svelte';
  import NotifBell from '$lib/NotifBell.svelte';

  onMount(() => {
    sendWsCommand('connectors.state');
  });

  const atem = $derived(live.atemState);
  const connecting = $derived(live.connectorStatus.atem === 'connecting');
  const bus = $derived(
    atem?.inputs.map((i) => ({ value: String(i.id), label: i.shortName || i.name })) ?? [],
  );
  const nameOf = (id: number | null | undefined) =>
    atem?.inputs.find((i) => i.id === id)?.name ?? '—';
  const shortOf = (id: number | null | undefined) =>
    atem?.inputs.find((i) => i.id === id)?.shortName ?? '—';
  const streaming = $derived(atem?.streaming === 'streaming');
  const recording = $derived(atem?.recording === 'recording');
  const stateLabel = (value: string | null | undefined) =>
    value ? $_(`switcher.state.${value}`) : $_('switcher.unsupported');
</script>

<PageHeader
  title={$_('switcher.title')}
  back={{ label: $_('switcher.back'), href: '/settings/connectors' }}
>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

{#if !atem && connecting}
  <div class="centre"><Spinner label={$_('switcher.connecting')} /></div>
{:else if !atem}
  <ErrorState title={$_('switcher.unavailable')} body={$_('switcher.notConnected')} />
{:else}
  <TransportDock
    label={$_('switcher.transport')}
    status={recording && streaming
      ? $_('camera.state.both')
      : recording
        ? $_('camera.state.recording')
        : streaming
          ? $_('camera.state.live')
          : $_('camera.state.standby')}
    current={`${atem.product} · ${$_('switcher.preview')} ${nameOf(atem.preview)}`}
    position={shortOf(atem.program)}
    actions={[
      {
        icon: 'next',
        label: $_('switcher.cut'),
        variant: 'primary',
        onclick: () => sendWsCommand('atem.cut'),
      },
      {
        icon: 'play',
        label: $_('switcher.auto'),
        onclick: () => sendWsCommand('atem.auto'),
      },
      {
        icon: recording ? 'stop' : 'record',
        label: recording ? $_('camera.stopRecord') : $_('camera.startRecord'),
        variant: recording ? 'stop' : 'default',
        disabled: !atem.recording || atem.recording === 'stopping',
        onclick: () => sendWsCommand(`atem.record.${recording ? 'stop' : 'start'}`),
      },
      {
        icon: streaming ? 'stop' : 'stream',
        label: streaming ? $_('camera.stopStream') : $_('camera.goLive'),
        variant: streaming ? 'stop' : 'default',
        disabled:
          !atem.streaming || atem.streaming === 'connecting' || atem.streaming === 'stopping',
        onclick: () => sendWsCommand(`atem.stream.${streaming ? 'stop' : 'start'}`),
      },
    ]}
  />

  <section class="overview">
    <OverviewCell
      label={$_('switcher.program')}
      value={shortOf(atem.program)}
      color="var(--status-live)"
    />
    <OverviewCell
      label={$_('switcher.stream')}
      value={stateLabel(atem.streaming)}
      color={streaming ? 'var(--status-live)' : 'var(--status-off)'}
      pulse={streaming}
      divider
    />
    <OverviewCell
      label={$_('switcher.record')}
      value={stateLabel(atem.recording)}
      color={recording ? 'var(--status-live)' : 'var(--status-off)'}
      divider
    />
  </section>

  <SectionLabel hint={nameOf(atem.program)}>{$_('switcher.program')}</SectionLabel>
  <List>
    <div class="pad">
      <Segmented
        label={$_('switcher.program')}
        value={String(atem.program)}
        options={bus}
        onchange={(v) => sendWsCommand('atem.program.set', { input: Number(v) })}
      />
    </div>
  </List>

  <SectionLabel hint={nameOf(atem.preview)}>{$_('switcher.preview')}</SectionLabel>
  <List>
    <div class="pad">
      <Segmented
        label={$_('switcher.preview')}
        value={String(atem.preview)}
        options={bus}
        onchange={(v) => sendWsCommand('atem.preview.set', { input: Number(v) })}
      />
    </div>
  </List>

  <SectionLabel>{$_('switcher.stream')}</SectionLabel>
  <List>
    <Row
      title={$_('switcher.destination')}
      detail={atem.streamService ?? $_('switcher.unsupported')}
      chevron={false}
      last
    />
  </List>

  <p class="note footer">{$_('switcher.note')}</p>
{/if}

<style>
  .overview {
    margin: 0 24px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border-block: 1px solid var(--border-hairline);
  }
  .centre {
    display: flex;
    justify-content: center;
    padding: 64px 24px;
  }
  .pad {
    padding: 14px 24px 16px;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
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
