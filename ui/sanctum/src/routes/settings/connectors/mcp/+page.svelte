<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { PageHeader, SectionLabel, Button, ToggleRow, ErrorState } from '@metocast/design-system';
  import {
    fetchConnectorConfig,
    saveConnectorConfig,
    getAppMode,
    getServerPort,
    getToken,
    getClientUrl,
    getClientToken,
    type McpConfig,
  } from '@metocast/core-client';
  import { pushToast } from '$lib/notifications.svelte';
  import NotifBell from '$lib/NotifBell.svelte';

  let config = $state<McpConfig | null>(null);
  let loadError = $state(false);
  let saving = $state(false);
  let address = $state('');
  let token = $state('');
  let copied = $state('');

  async function load() {
    loadError = false;
    try {
      config = await fetchConnectorConfig('mcp');
    } catch {
      loadError = true;
      return;
    }
    // Where an agent reaches this core: this machine in server mode, the remote one in client mode.
    try {
      if ((await getAppMode()) === 'client') {
        address = `${(await getClientUrl()) ?? ''}/mcp`;
        token = await getClientToken();
      } else {
        address = `http://127.0.0.1:${await getServerPort()}/mcp`;
        token = await getToken();
      }
    } catch {
      /* no host: the switches still work */
    }
  }

  onMount(load);

  const claudeCode = $derived(
    `claude mcp add --transport http metocast ${address} --header "Authorization: Bearer ${token}"`,
  );
  const claudeDesktop = $derived(
    JSON.stringify({
      mcpServers: {
        metocast: {
          command: 'npx',
          args: ['-y', 'mcp-remote', address, '--header', 'Authorization:${AUTH_HEADER}'],
          env: { AUTH_HEADER: `Bearer ${token}` },
        },
      },
    }),
  );

  async function copy(label: string, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = label;
      setTimeout(() => (copied = ''), 2000);
    } catch {
      /* clipboard blocked */
    }
  }

  async function save() {
    if (!config) return;
    saving = true;
    try {
      await saveConnectorConfig('mcp', config);
      config = await fetchConnectorConfig('mcp');
      pushToast({
        kind: $_('toast.connector'),
        source: $_('mcp.toast'),
        title: $_('mcp.saved'),
        tone: 'ok',
      });
    } catch (e) {
      pushToast({
        kind: $_('toast.connector'),
        source: $_('mcp.toast'),
        title: $_('mcp.saveFailed'),
        body: String(e),
        tone: 'error',
      });
    } finally {
      saving = false;
    }
  }
</script>

<PageHeader title={$_('mcp.title')} back={{ label: $_('conn.back'), href: '/settings/connectors' }}>
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
    <ToggleRow label={$_('mcp.enabled')} sub={$_('mcp.enabledSub')} bind:checked={config.enabled} />
    <ToggleRow
      label={$_('mcp.readOnly')}
      sub={$_('mcp.readOnlySub')}
      bind:checked={config.readOnly}
    />
    <ToggleRow
      label={$_('mcp.allowLan')}
      sub={$_('mcp.allowLanSub')}
      bind:checked={config.allowLan}
    />
    <Button variant="primary" block loading={saving} onclick={save}>{$_('mcp.save')}</Button>

    {#if address}
      <SectionLabel hint={$_('mcp.tokenNote')}>{$_('mcp.connect')}</SectionLabel>
      <p class="address"><span>{$_('mcp.address')}</span> <code>{address}</code></p>
      <Button variant="secondary" onclick={() => copy('code', claudeCode)}>
        {copied === 'code' ? $_('mcp.copied') : $_('mcp.copyClaudeCode')}
      </Button>
      <Button variant="secondary" onclick={() => copy('desktop', claudeDesktop)}>
        {copied === 'desktop' ? $_('mcp.copied') : $_('mcp.copyClaudeDesktop')}
      </Button>
    {/if}
  </div>
{/if}

<style>
  .form {
    display: grid;
    gap: 14px;
    max-width: 640px;
    padding: 0 24px 56px;
  }
  .address {
    display: flex;
    gap: 10px;
    align-items: baseline;
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .address code {
    font-size: 13px;
    color: var(--text-primary);
    word-break: break-all;
  }
</style>
