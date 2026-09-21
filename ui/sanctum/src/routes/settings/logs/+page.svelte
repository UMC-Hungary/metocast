<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    PageHeader,
    SectionLabel,
    List,
    Row,
    Segmented,
    Button,
    Spinner,
    ErrorState,
    EmptyState,
    Dialog,
    TextIcon,
  } from '@metocast/design-system';
  import {
    fetchApplicationLog,
    clearApplicationLog,
    getAppMode,
    hostCapabilities,
    openApplicationLog,
  } from '@metocast/core-client';
  import { hasToken } from '$lib/core';
  import { pushToast } from '$lib/notifications.svelte';
  import NotifBell from '$lib/NotifBell.svelte';

  type Level = 'info' | 'warn' | 'error' | 'other';
  type Filter = 'all' | 'info' | 'warn' | 'error';

  let path = $state('');
  let content = $state('');
  let filter = $state<Filter>('all');
  let loading = $state(true);
  let clearing = $state(false);
  let failure = $state('');
  let confirmOpen = $state(false);
  let mode = $state<'server' | 'client'>('server');

  // The core answers with its own log, so a client device reads the server's file
  // — the local one it could open through the host would be the wrong log.
  const canOpenFile = $derived(hostCapabilities.logs && mode === 'server');

  const lines = $derived(
    content
      .split(/\r?\n/)
      .filter(Boolean)
      .map((text, index) => ({ id: index, text, level: classify(text) })),
  );
  const counts = $derived({
    all: lines.length,
    info: lines.filter((l) => l.level === 'info').length,
    warn: lines.filter((l) => l.level === 'warn').length,
    error: lines.filter((l) => l.level === 'error').length,
  });
  const visible = $derived(filter === 'all' ? lines : lines.filter((l) => l.level === filter));
  const options = $derived(
    (['all', 'info', 'warn', 'error'] as Filter[]).map((value) => ({
      value,
      label: `${$_(`logs.filters.${value}`)} ${counts[value]}`,
    })),
  );

  function classify(line: string): Level {
    switch (line.match(/\b(ERROR|WARN|WARNING|INFO)\b/)?.[1]) {
      case 'ERROR':
        return 'error';
      case 'WARN':
      case 'WARNING':
        return 'warn';
      case 'INFO':
        return 'info';
      default:
        return 'other';
    }
  }

  function fail(title: string, e: unknown) {
    pushToast({
      kind: $_('logs.title'),
      source: 'Metocast',
      title,
      body: String(e),
      tone: 'error',
    });
  }

  async function load() {
    loading = true;
    failure = '';
    // Reached by URL without a token: say so now rather than waiting out the
    // client's token timeout on a request the core would refuse anyway.
    if (!hasToken()) {
      failure = $_('logs.noToken');
      loading = false;
      return;
    }
    try {
      ({ path, content } = await fetchApplicationLog());
    } catch (e) {
      failure = String(e);
    } finally {
      loading = false;
    }
  }

  async function clear() {
    confirmOpen = false;
    clearing = true;
    try {
      await clearApplicationLog();
      await load();
      pushToast({
        kind: $_('logs.title'),
        source: 'Metocast',
        title: $_('logs.toasts.removed'),
        tone: 'ok',
      });
    } catch (e) {
      fail($_('logs.toasts.removeFailed'), e);
    } finally {
      clearing = false;
    }
  }

  async function copyPath() {
    try {
      await navigator.clipboard.writeText(path);
      pushToast({
        kind: $_('logs.title'),
        source: 'Metocast',
        title: $_('logs.toasts.pathCopied'),
        tone: 'ok',
      });
    } catch (e) {
      fail($_('logs.toasts.copyFailed'), e);
    }
  }

  function download() {
    const url = URL.createObjectURL(new Blob([content], { type: 'text/plain' }));
    const link = Object.assign(document.createElement('a'), {
      href: url,
      download: 'metocast.log',
    });
    link.click();
    URL.revokeObjectURL(url);
  }

  async function openFile() {
    try {
      await openApplicationLog();
    } catch (e) {
      fail($_('logs.toasts.openFailed'), e);
    }
  }

  onMount(async () => {
    mode = (await getAppMode().catch(() => 'server')) ?? 'server';
    await load();
  });
</script>

<PageHeader title={$_('logs.title')} back={{ label: $_('settings.back'), href: '/settings' }}>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

{#if loading && !content}
  <div class="centre"><Spinner label={$_('logs.loading')} /></div>
{:else if failure}
  <ErrorState
    title={$_('logs.unavailable')}
    body={failure}
    retryLabel={$_('logs.retry')}
    onretry={load}
  />
{:else}
  <SectionLabel hint={mode}>{$_('logs.fileSection')}</SectionLabel>
  <List>
    <Row title={$_('logs.file')} meta={path} chevron={false} last>
      {#snippet icon()}<TextIcon char="≡" />{/snippet}
      {#snippet control()}
        <Button variant="secondary" compact onclick={copyPath}>{$_('logs.copyPath')}</Button>
      {/snippet}
    </Row>
  </List>

  <div class="toolbar">
    <Segmented
      compact
      label={$_('logs.filters.label')}
      value={filter}
      {options}
      onchange={(value) => (filter = value)}
    />
    <div class="actions">
      <Button
        variant="secondary"
        compact
        onclick={load}
        {loading}
        loadingLabel={$_('logs.loading')}
      >
        {$_('logs.refresh')}
      </Button>
      <Button variant="secondary" compact onclick={download} disabled={!content}>
        {$_('logs.download')}
      </Button>
      {#if canOpenFile}
        <Button variant="secondary" compact onclick={openFile}>{$_('logs.open')}</Button>
      {/if}
      <Button variant="danger" compact onclick={() => (confirmOpen = true)} disabled={clearing}>
        {clearing ? $_('logs.removing') : $_('logs.remove')}
      </Button>
    </div>
  </div>

  {#if lines.length === 0}
    <EmptyState title={$_('logs.empty')} hint={$_('logs.emptyHint')} />
  {:else if visible.length === 0}
    <EmptyState title={$_('logs.noMatches')} />
  {:else}
    <div class="surface" aria-live="polite">
      {#each visible as line (line.id)}
        <p class="line {line.level}">{line.text}</p>
      {/each}
    </div>
  {/if}
{/if}

<Dialog bind:open={confirmOpen} title={$_('logs.remove')} eyebrow={$_('logs.title')}>
  <p class="note">{$_('logs.removeConfirm')}</p>
  {#snippet footer()}
    <Button onclick={() => (confirmOpen = false)}>{$_('logs.cancel')}</Button>
    <Button variant="danger" onclick={clear}>{$_('logs.remove')}</Button>
  {/snippet}
</Dialog>

<style>
  .centre {
    display: flex;
    justify-content: center;
    padding: 64px 24px;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 14px 24px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .surface {
    margin: 0 24px 96px;
    max-height: 60vh;
    overflow: auto;
    border: 1px solid var(--border-hairline);
    padding: 10px 0;
  }
  .line {
    margin: 0;
    padding: 2px 12px;
    border-left: 3px solid transparent;
    font-family: var(--font-label);
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .line.info {
    color: var(--text-primary);
  }
  .line.warn {
    border-left-color: var(--status-warn);
    color: var(--status-warn);
  }
  .line.error {
    border-left-color: var(--status-error);
    color: var(--status-error);
  }
  .note {
    margin: 0;
    font-size: 14px;
    line-height: 1.45;
    color: var(--text-muted);
  }

  @media (min-width: 760px) {
    .surface {
      margin-bottom: 32px;
      max-height: calc(100vh - 320px);
    }
  }
</style>
