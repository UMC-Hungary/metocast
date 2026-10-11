<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    PageHeader,
    SectionLabel,
    List,
    Row,
    Field,
    Button,
    Select,
    ToggleRow,
    TextIcon,
    ErrorState,
    type SelectOption,
  } from '@metocast/design-system';
  import {
    discoverRodecasterAudio,
    fetchConnectorConfig,
    saveConnectorConfig,
    verifyOpenAiKey,
    type OpenAiConfig,
    type OpenAiVerifyResult,
    type RodecasterAudioSource,
  } from '@metocast/core-client';
  import { live } from '$lib/live.svelte';
  import { pushToast } from '$lib/notifications.svelte';
  import NotifBell from '$lib/NotifBell.svelte';
  import CreditLine from '$lib/CreditLine.svelte';

  let config = $state<OpenAiConfig | null>(null);
  let apiKey = $state('');
  let adminKey = $state('');
  let ingestionUrl = $state('');
  let lag = $state('3200');
  let topUpAmount = $state('');
  let topUpDate = $state(new Date().toISOString().slice(0, 10));
  let verifying = $state(false);
  let verified = $state<OpenAiVerifyResult | null>(null);
  let saving = $state(false);
  let loadError = $state(false);

  const sourceKey = (s: RodecasterAudioSource) =>
    s.kind === 'mainMix' ? 'mainMix' : `fader-${s.number}`;
  const sources = $derived(live.rodecasterAudioDiscovery?.endpoint?.sources ?? []);
  const sourceOptions = $derived<SelectOption[]>(
    sources.length
      ? sources.map((s) => ({ value: sourceKey(s.identity), label: s.label }))
      : config
        ? [{ value: sourceKey(config.source), label: sourceKey(config.source) }]
        : [],
  );
  let sourceValue = $state('mainMix');

  async function load() {
    loadError = false;
    discoverRodecasterAudio();
    try {
      config = await fetchConnectorConfig('openai');
      lag = String(config.lagMs);
      sourceValue = sourceKey(config.source);
    } catch {
      loadError = true;
    }
  }

  onMount(load);

  async function verify() {
    verifying = true;
    try {
      verified = await verifyOpenAiKey(apiKey);
    } catch (e) {
      verified = { result: 'noAccess', message: String(e) };
    } finally {
      verifying = false;
    }
  }

  function addTopUp() {
    const amountUsd = Number(topUpAmount);
    if (!config || !(amountUsd > 0) || !topUpDate) return;
    config.topUps = [...config.topUps, { amountUsd, date: topUpDate }].sort((a, b) =>
      a.date.localeCompare(b.date),
    );
    topUpAmount = '';
  }

  async function save() {
    if (!config) return;
    saving = true;
    const source =
      sources.find((s) => sourceKey(s.identity) === sourceValue)?.identity ?? config.source;
    try {
      // Blank secrets keep what the core stores.
      await saveConnectorConfig('openai', {
        ...config,
        apiKey,
        adminKey,
        captionIngestionUrl: ingestionUrl,
        source,
        lagMs: Number(lag) || 0,
      });
      config = await fetchConnectorConfig('openai');
      apiKey = adminKey = ingestionUrl = '';
      pushToast({
        kind: $_('toast.connector'),
        source: 'OpenAI',
        title: $_('conn.saved'),
        tone: 'ok',
      });
    } catch (e) {
      pushToast({
        kind: $_('toast.connector'),
        source: 'OpenAI',
        title: $_('conn.toast.saveFail'),
        body: String(e),
        tone: 'error',
      });
    } finally {
      saving = false;
    }
  }
</script>

<PageHeader
  title={$_('openai.title')}
  back={{ label: $_('rodecaster.back'), href: '/settings/connectors' }}
>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

{#if loadError}
  <ErrorState
    title={$_('common.errorTitle')}
    body={$_('common.errorBody')}
    retryLabel={$_('common.retry')}
    onretry={load}
  />
{:else if config}
  <div class="form">
    <ToggleRow
      label={$_('openai.enabled')}
      sub={$_('openai.enabledSub')}
      bind:checked={config.enabled}
    />

    <SectionLabel>{$_('openai.keys')}</SectionLabel>
    <Field
      label={$_('openai.apiKey')}
      type="password"
      bind:value={apiKey}
      placeholder={config.apiKeySet ? '••••••••' : 'sk-…'}
      hint={verified ? `${$_(`openai.verify.${verified.result}`)} ${verified.message}` : ''}
    />
    <Button variant="secondary" compact loading={verifying} onclick={verify}>
      {$_('openai.verify.button')}
    </Button>
    <Field
      label={$_('openai.adminKey')}
      type="password"
      bind:value={adminKey}
      placeholder={config.adminKeySet ? '••••••••' : ''}
      hint={$_('openai.adminKeyHint')}
    />

    <SectionLabel hint={$_('openai.topUpsHint')}>{$_('openai.topUps')}</SectionLabel>
    <CreditLine />
    {#if config.topUps.length}
      <List>
        {#each config.topUps as topUp, i (`${topUp.date}-${i}`)}
          <Row
            title={`$${topUp.amountUsd.toFixed(2)}`}
            meta={topUp.date}
            chevron={false}
            last={i === config.topUps.length - 1}
          >
            {#snippet icon()}<TextIcon char="$" />{/snippet}
            {#snippet control()}
              <Button
                variant="quiet"
                compact
                onclick={() => config && (config.topUps = config.topUps.filter((_, j) => j !== i))}
              >
                {$_('openai.remove')}
              </Button>
            {/snippet}
          </Row>
        {/each}
      </List>
    {/if}
    <div class="row">
      <Field label={$_('openai.amount')} type="number" bind:value={topUpAmount} />
      <label class="date">
        <span>{$_('openai.date')}</span>
        <input type="date" bind:value={topUpDate} />
      </label>
      <Button variant="secondary" compact onclick={addTopUp}>{$_('openai.add')}</Button>
    </div>

    <SectionLabel>{$_('openai.audio')}</SectionLabel>
    <Select label={$_('openai.source')} options={sourceOptions} bind:value={sourceValue} />
    <Field label={$_('openai.lag')} type="number" bind:value={lag} hint={$_('openai.lagHint')} />

    <SectionLabel>YouTube</SectionLabel>
    <ToggleRow
      label={$_('openai.liveCaptions')}
      sub={$_('openai.liveCaptionsSub')}
      bind:checked={config.youtubeLiveCaptions}
    />
    <Field
      label={$_('openai.ingestionUrl')}
      type="password"
      bind:value={ingestionUrl}
      placeholder={config.captionIngestionUrlSet
        ? '••••••••'
        : 'http://upload.youtube.com/closedcaption?cid=…'}
      hint={$_('openai.ingestionUrlHint')}
    />
    <ToggleRow
      label={$_('openai.autoCaptions')}
      sub={$_('openai.autoCaptionsSub')}
      bind:checked={config.autoYoutubeCaptions}
    />

    <Button variant="primary" block loading={saving} onclick={save}>{$_('openai.save')}</Button>
  </div>
{/if}

<style>
  .form {
    display: grid;
    gap: 14px;
    max-width: 640px;
    padding: 0 24px 56px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 10px;
    align-items: end;
  }
  .date {
    display: grid;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .date input {
    font: inherit;
    font-size: 14px;
    padding: 10px 12px;
    color: var(--text-primary);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--text-primary) 16%, transparent);
  }
</style>
