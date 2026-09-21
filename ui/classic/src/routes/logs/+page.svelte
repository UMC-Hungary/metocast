<script lang="ts">
  import { onMount } from 'svelte';
  import {
    downloadApplicationLog,
    getApplicationLogPath,
    hostCapabilities,
    openApplicationLog,
    pickSavePath,
    readApplicationLog,
    removeApplicationLog,
  } from '@metocast/core-client';
  import { Copy, Download, ExternalLink, RefreshCw, Trash2 } from 'lucide-svelte';
  import { toast } from 'svelte-sonner';
  import { _ } from 'svelte-i18n';
  import {
    ApplicationLogPathSchema,
    ApplicationLogTextSchema,
  } from '@metocast/core-client/schemas/logs';

  type LogFilter = 'all' | 'info' | 'warn' | 'error';
  type LogLineLevel = Exclude<LogFilter, 'all'> | 'other';

  type LogLine = {
    id: string;
    text: string;
    level: LogLineLevel;
  };

  const filters: { id: LogFilter; labelKey: string }[] = [
    { id: 'all', labelKey: 'logsPage.filters.all' },
    { id: 'info', labelKey: 'logsPage.filters.info' },
    { id: 'warn', labelKey: 'logsPage.filters.warn' },
    { id: 'error', labelKey: 'logsPage.filters.error' },
  ];

  let rawLog = $state('');
  let logPath = $state('');
  let selectedFilter = $state<LogFilter>('all');
  let isLoading = $state(false);
  let isDownloading = $state(false);
  let isRemoving = $state(false);
  let desktopAvailable = $state(false);
  let errorMessage = $state<string | null>(null);

  let logLines = $derived(parseLogLines(rawLog));
  let visibleLines = $derived(
    selectedFilter === 'all' ? logLines : logLines.filter((line) => line.level === selectedFilter),
  );
  let counts = $derived({
    all: logLines.length,
    info: logLines.filter((line) => line.level === 'info').length,
    warn: logLines.filter((line) => line.level === 'warn').length,
    error: logLines.filter((line) => line.level === 'error').length,
  });

  onMount(() => {
    desktopAvailable = hostCapabilities.logs;
    void loadLog();
  });

  async function loadLog() {
    if (!desktopAvailable) {
      errorMessage = $_('logsPage.desktopOnly');
      return;
    }

    isLoading = true;
    errorMessage = null;
    try {
      const [content, path] = await Promise.all([readApplicationLog(), getApplicationLogPath()]);
      rawLog = ApplicationLogTextSchema.parse(content);
      logPath = ApplicationLogPathSchema.parse(path);
    } catch (e) {
      errorMessage = errorText(e);
    } finally {
      isLoading = false;
    }
  }

  async function openLog() {
    if (!desktopAvailable) {
      toast.error($_('logsPage.toasts.openFailed'), {
        description: $_('logsPage.desktopOnly'),
      });
      return;
    }

    try {
      await openApplicationLog();
      toast.success($_('logsPage.toasts.opened'));
    } catch (e) {
      toast.error($_('logsPage.toasts.openFailed'), {
        description: errorText(e),
      });
    }
  }

  async function downloadLog() {
    if (!desktopAvailable) {
      toast.error($_('logsPage.toasts.downloadFailed'), {
        description: $_('logsPage.desktopOnly'),
      });
      return;
    }

    isDownloading = true;
    try {
      const destination = await pickSavePath({
        defaultPath: 'metocast.log',
        filters: [{ name: $_('logsPage.logFileFilter'), extensions: ['log', 'txt'] }],
      });
      if (!destination) return;

      await downloadApplicationLog(destination);
      toast.success($_('logsPage.toasts.downloaded'));
    } catch (e) {
      toast.error($_('logsPage.toasts.downloadFailed'), {
        description: errorText(e),
      });
    } finally {
      isDownloading = false;
    }
  }

  async function removeLog() {
    if (!desktopAvailable) {
      toast.error($_('logsPage.toasts.removeFailed'), {
        description: $_('logsPage.desktopOnly'),
      });
      return;
    }

    if (!window.confirm($_('logsPage.removeConfirm'))) return;

    isRemoving = true;
    try {
      await removeApplicationLog();
      toast.success($_('logsPage.toasts.removed'));
      await loadLog();
    } catch (e) {
      toast.error($_('logsPage.toasts.removeFailed'), {
        description: errorText(e),
      });
    } finally {
      isRemoving = false;
    }
  }

  async function copyLogPath() {
    if (!desktopAvailable) {
      toast.error($_('logsPage.toasts.copyFailed'), {
        description: $_('logsPage.desktopOnly'),
      });
      return;
    }

    try {
      if (!navigator.clipboard) {
        throw new Error($_('logsPage.toasts.clipboardUnavailable'));
      }
      const path = logPath || (await getApplicationLogPath());
      await navigator.clipboard.writeText(path);
      logPath = path;
      toast.success($_('logsPage.toasts.pathCopied'));
    } catch (e) {
      toast.error($_('logsPage.toasts.copyFailed'), {
        description: errorText(e),
      });
    }
  }

  function parseLogLines(content: string): LogLine[] {
    return content
      .split(/\r?\n/)
      .filter((line) => line.length > 0)
      .map((line, index) => ({
        id: `${index}-${line.slice(0, 32)}`,
        text: line,
        level: classifyLine(line),
      }));
  }

  function classifyLine(line: string): LogLineLevel {
    const match = line.match(/\b(ERROR|WARN|WARNING|INFO)\b/);
    if (!match) return 'other';

    switch (match[1]) {
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

  function errorText(error: unknown): string {
    if (error instanceof Error) return error.message;
    return String(error);
  }
</script>

<svelte:head>
  <title>{$_('logsPage.title')} — Sermon Helper</title>
</svelte:head>

<div class="logs-page">
  <div class="page-header">
    <div>
      <h1>{$_('logsPage.title')}</h1>
      {#if logPath}
        <p class="log-path">{logPath}</p>
      {/if}
    </div>
    <div class="header-actions">
      <button
        class="icon-button"
        type="button"
        onclick={loadLog}
        disabled={isLoading || !desktopAvailable}
        title={$_('logsPage.refresh')}
      >
        <RefreshCw size={16} />
        <span>{isLoading ? $_('logsPage.loading') : $_('logsPage.refresh')}</span>
      </button>
      <button
        class="icon-button"
        type="button"
        onclick={copyLogPath}
        disabled={!desktopAvailable}
        title={$_('logsPage.copyPath')}
      >
        <Copy size={16} />
        <span>{$_('logsPage.copyPath')}</span>
      </button>
      <button
        class="icon-button"
        type="button"
        onclick={downloadLog}
        disabled={!desktopAvailable || isDownloading}
        title={$_('logsPage.downloadLog')}
      >
        <Download size={16} />
        <span>{isDownloading ? $_('logsPage.downloading') : $_('logsPage.downloadLog')}</span>
      </button>
      <button
        class="icon-button danger"
        type="button"
        onclick={removeLog}
        disabled={!desktopAvailable || isRemoving}
        title={$_('logsPage.removeLog')}
      >
        <Trash2 size={16} />
        <span>{isRemoving ? $_('logsPage.removing') : $_('logsPage.removeLog')}</span>
      </button>
      <button
        class="icon-button primary"
        type="button"
        onclick={openLog}
        disabled={!desktopAvailable}
        title={$_('logsPage.openLog')}
      >
        <ExternalLink size={16} />
        <span>{$_('logsPage.openLog')}</span>
      </button>
    </div>
  </div>

  <div class="filter-row" role="tablist" aria-label={$_('logsPage.filters.label')}>
    {#each filters as filter (filter.id)}
      <button
        type="button"
        role="tab"
        aria-selected={selectedFilter === filter.id}
        class:active={selectedFilter === filter.id}
        onclick={() => {
          selectedFilter = filter.id;
        }}
      >
        <span>{$_(filter.labelKey)}</span>
        <span class="count">{counts[filter.id]}</span>
      </button>
    {/each}
  </div>

  {#if errorMessage}
    <div class="status-panel error">{errorMessage}</div>
  {:else if isLoading && rawLog.length === 0}
    <div class="status-panel">{$_('logsPage.loading')}</div>
  {:else if logLines.length === 0}
    <div class="status-panel">{$_('logsPage.empty')}</div>
  {:else if visibleLines.length === 0}
    <div class="status-panel">{$_('logsPage.noMatches')}</div>
  {:else}
    <div class="log-surface" aria-live="polite">
      {#each visibleLines as line (line.id)}
        <div class="log-line level-{line.level}">
          {line.text}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .logs-page {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 1000px;
    min-width: 0;
    min-height: calc(100vh - var(--titlebar-height) - 3rem);
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    flex-wrap: wrap;
  }

  .page-header > div:first-child {
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-size: 1.5rem;
  }

  .log-path {
    margin: 0.375rem 0 0;
    color: var(--text-secondary);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    overflow-wrap: anywhere;
  }

  .header-actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.375rem;
    min-height: 2.125rem;
    padding: 0 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: transparent;
    color: var(--text-primary);
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
  }

  .icon-button span {
    overflow-wrap: anywhere;
  }

  .icon-button:hover:not(:disabled) {
    background: var(--nav-item-hover);
  }

  .icon-button:disabled {
    cursor: not-allowed;
    opacity: 0.65;
  }

  .icon-button.primary {
    border-color: var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
  }

  .icon-button.danger {
    border-color: var(--status-err-text);
    color: var(--status-err-text);
  }

  .icon-button.danger:hover:not(:disabled) {
    background: var(--status-err-bg);
  }

  .filter-row {
    display: inline-flex;
    align-self: flex-start;
    gap: 0.25rem;
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--glass-card-bg);
  }

  .filter-row button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    min-width: 0;
    min-height: 2rem;
    padding: 0 0.75rem;
    border: none;
    border-radius: 0.375rem;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
  }

  .filter-row button.active {
    background: var(--nav-item-active-bg);
    color: var(--nav-item-active-text);
  }

  .count {
    min-width: 1.25rem;
    padding: 0.05rem 0.35rem;
    border-radius: 9999px;
    background: var(--content-bg);
    color: inherit;
    font-size: 0.72rem;
    text-align: center;
  }

  .log-surface {
    flex: 1;
    min-height: 420px;
    max-height: calc(100vh - var(--titlebar-height) - 12rem);
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--glass-card-bg);
    padding: 0.75rem 0;
  }

  .log-line {
    padding: 0.125rem 0.875rem;
    border-left: 3px solid transparent;
    color: var(--text-secondary);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .log-line.level-info {
    color: var(--text-primary);
  }

  .log-line.level-warn {
    border-left-color: var(--status-warn-text);
    background: var(--status-warn-bg);
    color: var(--status-warn-text);
  }

  .log-line.level-error {
    border-left-color: var(--status-err-text);
    background: var(--status-err-bg);
    color: var(--status-err-text);
  }

  .status-panel {
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    padding: 1rem;
    color: var(--text-secondary);
    font-size: 0.875rem;
  }

  .status-panel.error {
    border-color: var(--status-err-text);
    background: var(--status-err-bg);
    color: var(--status-err-text);
  }

  @media (max-width: 760px) {
    .header-actions,
    .filter-row {
      width: 100%;
    }

    .header-actions {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
    }

    .filter-row {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      align-self: stretch;
    }
  }
</style>
