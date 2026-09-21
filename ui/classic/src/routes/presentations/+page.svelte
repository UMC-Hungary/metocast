<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { pickDirectory } from '@metocast/core-client';
  import { pptFilter, pptResults, pptFolders, keynoteStatus } from '$lib/stores/presentations.js';
  import { presenterState, useWebPresenter, connectedClients } from '$lib/stores/presenter.js';
  import { appReady, localNetworkUrl, authToken, serverPort } from '$lib/stores/server-url.js';
  import { lastWsMessage, wsStatus } from '$lib/stores/ws.js';
  import { sendWsCommand } from '@metocast/core-client';
  import { onMount, onDestroy, untrack } from 'svelte';
  import {
    listFolders,
    addFolder,
    removeFolder,
    searchFiles,
    keynoteCloseAll,
  } from '@metocast/core-client';
  import type {
    BibleReference,
    Event as ServiceEvent,
    EventSummary,
  } from '@metocast/core-client/schemas/event';
  import type { WsMessage } from '@metocast/core-client/schemas/ws-messages';
  import SlideEditorModal from '$lib/components/presentations/SlideEditorModal.svelte';

  type BibleReferenceKind = 'textus' | 'leckio';

  let addingFolder = $state(false);
  let folderError = $state('');
  let foldersFetched = $state(false);
  let eventsFetched = $state(false);
  let eventsLoading = $state(false);
  let eventSummaries = $state<EventSummary[]>([]);
  let selectedEventId = $state('');
  let selectedEvent = $state<ServiceEvent | null>(null);
  let selectedEventLoading = $state(false);
  let copySuccess = $state(false);
  let pingingClient = $state<string | null>(null);
  let now = $state(Date.now());
  let clockTimer: ReturnType<typeof setInterval>;
  let slideEditorOpen = $state(false);
  const textusReference = $derived(findBibleReference(selectedEvent, 'textus'));
  const leckioReference = $derived(findBibleReference(selectedEvent, 'leckio'));

  onMount(() => {
    sendWsCommand('clients.list');
    clockTimer = setInterval(() => {
      now = Date.now();
    }, 10_000);
  });

  onDestroy(() => {
    clearInterval(clockTimer);
  });

  function formatDuration(isoString: string): string {
    const diffMs = now - new Date(isoString).getTime();
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  }

  function parseBrowser(ua: string | null): string {
    if (!ua) return 'Unknown';
    if (ua.includes('Tauri')) return 'Tauri App';
    if (ua.includes('Edg/')) return 'Edge';
    if (ua.includes('Chrome/')) return 'Chrome';
    if (ua.includes('Firefox/')) return 'Firefox';
    if (ua.includes('Safari/') && !ua.includes('Chrome')) return 'Safari';
    return 'Browser';
  }

  function latencyClass(ms: number | null): string {
    if (ms === null) return 'latency-unknown';
    if (ms < 30) return 'latency-good';
    if (ms < 100) return 'latency-ok';
    return 'latency-slow';
  }

  $effect(() => {
    if ($appReady && !foldersFetched) {
      foldersFetched = true;
      listFolders().then((folders) => pptFolders.set(folders));
    }
  });

  $effect(() => {
    if ($wsStatus !== 'connected') {
      eventsFetched = false;
      return;
    }
    if ($appReady && $useWebPresenter && !eventsFetched) {
      eventsFetched = true;
      loadPresenterEvents();
    }
  });

  $effect(() => {
    if (selectedEventId) {
      loadSelectedEvent(selectedEventId);
    } else {
      selectedEvent = null;
      selectedEventLoading = false;
    }
  });

  $effect(() => {
    const message = $lastWsMessage;
    if (!message) return;
    untrack(() => handlePresenterWsMessage(message));
  });

  function handlePresenterWsMessage(message: WsMessage) {
    if (message.type === 'events.presenter_list') {
      eventsLoading = false;
      eventSummaries = message.events;
      const nextSelectedEventId = message.selectedEventId ?? '';
      if (!eventSummaries.some((event) => event.id === selectedEventId)) {
        selectedEventId = nextSelectedEventId;
      }
    } else if (message.type === 'events.get') {
      if (message.event.id === selectedEventId) {
        selectedEvent = message.event;
        selectedEventLoading = false;
      }
    } else if (message.type === 'event.changed' && $useWebPresenter) {
      loadPresenterEvents();
      if (selectedEventId && message.data.record.id === selectedEventId) {
        loadSelectedEvent(selectedEventId);
      }
    }
  }

  function loadPresenterEvents() {
    eventsLoading = true;
    if (!sendWsCommand('events.presenter_list')) {
      console.error('Failed to request presenter events: WebSocket is not connected');
      eventSummaries = [];
      selectedEventId = '';
      eventsLoading = false;
    }
  }

  function loadSelectedEvent(eventId: string) {
    selectedEventLoading = true;
    selectedEvent = null;
    if (!sendWsCommand('events.get', { id: eventId })) {
      console.error('Failed to request presenter event details: WebSocket is not connected');
      selectedEventLoading = false;
    }
  }

  function findBibleReference(
    event: ServiceEvent | null,
    referenceType: BibleReferenceKind,
  ): BibleReference | null {
    return event?.bibleReferences.find((reference) => reference.type === referenceType) ?? null;
  }

  function hasBibleVerses(reference: BibleReference | null): boolean {
    return (reference?.verses.length ?? 0) > 0;
  }

  function formatEventOption(event: EventSummary): string {
    const eventDate = new Date(event.dateTime);
    const dateLabel = eventDate.toLocaleDateString([], { month: 'short', day: 'numeric' });
    const timeLabel = eventDate.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    const speaker = event.speaker ? ` · ${event.speaker}` : '';
    return `${dateLabel} ${timeLabel} · ${event.title}${speaker}`;
  }

  function handleEventChange(event: Event) {
    const target = event.currentTarget;
    if (target instanceof HTMLSelectElement) {
      selectedEventId = target.value;
    }
  }

  function handlePresentBible(referenceType: BibleReferenceKind) {
    if (!selectedEventId) return;
    sendWsCommand('presenter.load_bible_reference', {
      event_id: selectedEventId,
      reference_type: referenceType,
    });
  }

  // ── Presenter URL ─────────────────────────────────────────────────────────

  function buildPresenterUrl(): string {
    const base = $localNetworkUrl || 'http://localhost:3737';
    const token = encodeURIComponent($authToken);
    return `${base}/presenter?token=${token}`;
  }

  async function copyPresenterUrl() {
    try {
      await navigator.clipboard.writeText(buildPresenterUrl());
      copySuccess = true;
      setTimeout(() => {
        copySuccess = false;
      }, 2000);
    } catch {
      // ignore clipboard errors
    }
  }

  function buildIframeUrl(): string {
    const token = encodeURIComponent($authToken);
    return `${window.location.origin}/presenter?token=${token}&wsPort=${$serverPort}`;
  }

  function pingClient(clientId: string) {
    pingingClient = clientId;
    sendWsCommand('clients.ping', { client_id: clientId });
    setTimeout(() => {
      pingingClient = null;
    }, 2000);
  }

  // ── Folder management ────────────────────────────────────────────────────

  async function handleAddFolder() {
    addingFolder = true;
    folderError = '';
    try {
      const selected = await pickDirectory();
      if (!selected || typeof selected !== 'string') return;
      const name = selected.split('/').pop() ?? selected;
      const folder = await addFolder(selected, name);
      pptFolders.update((list) => [...list, folder]);
    } catch (e) {
      folderError = e instanceof Error ? e.message : String(e);
    } finally {
      addingFolder = false;
    }
  }

  async function handleRemoveFolder(id: string) {
    await removeFolder(id);
    pptFolders.update((list) => list.filter((f) => f.id !== id));
  }

  // ── File search ──────────────────────────────────────────────────────────

  function appendDigit(digit: string) {
    pptFilter.update((f) => f + digit);
    triggerSearch();
  }

  function handleClear() {
    pptFilter.set('');
    triggerSearch();
  }

  async function triggerSearch() {
    const filter = $pptFilter;
    if (!sendWsCommand('ppt.search', { filter })) {
      const files = await searchFiles(filter);
      pptResults.set(files);
    }
  }

  // ── File open ────────────────────────────────────────────────────────────

  function handleOpenSlot(path: string) {
    sendWsCommand('presentation.open', { file_path: path });
  }

  // ── Navigation ───────────────────────────────────────────────────────────

  function handleNext() {
    sendWsCommand('presentation.next');
  }
  function handlePrev() {
    sendWsCommand('presentation.prev');
  }
  function handleFirst() {
    sendWsCommand('presentation.first');
  }
  function handleLast() {
    sendWsCommand('presentation.last');
  }
  function handleStart() {
    sendWsCommand('presentation.start');
  }
  function handleStopOrUnload() {
    sendWsCommand($useWebPresenter ? 'presentation.close' : 'presentation.stop');
  }

  function handleToggleMute() {
    sendWsCommand($presenterState.muted ? 'presentation.unmute' : 'presentation.mute');
  }
  async function handleCloseAll() {
    if (!sendWsCommand('keynote.close_all')) await keynoteCloseAll();
  }
</script>

<div class="presentations-page">
  <h1>{$_('presentations.title')}</h1>

  <!-- Folder Manager -->
  <section class="section">
    <h2>{$_('presentations.folders.title')}</h2>
    <ul class="folder-list">
      {#each $pptFolders as folder (folder.id)}
        <li class="folder-item">
          <span class="folder-name" title={folder.path}>{folder.name}</span>
          <span class="folder-path">{folder.path}</span>
          <button
            onclick={() => handleRemoveFolder(folder.id)}
            aria-label={$_('presentations.folders.remove')}
          >
            ✕
          </button>
        </li>
      {/each}
    </ul>
    <button class="btn-primary" onclick={handleAddFolder} disabled={addingFolder}>
      {addingFolder ? $_('common.loading') : $_('presentations.folders.add')}
    </button>
    {#if folderError}<p class="folder-error">{folderError}</p>{/if}
  </section>

  <!-- File Search -->
  <section class="section">
    <h2>{$_('presentations.search.title')}</h2>
    <div class="filter-display">
      <span class="filter-value">{$pptFilter || '—'}</span>
    </div>
    <div class="digit-pad">
      {#each ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'] as digit (digit)}
        <button class="digit-btn" onclick={() => appendDigit(digit)}>{digit}</button>
      {/each}
      <button class="digit-btn clear-btn" onclick={handleClear}>CLR</button>
    </div>
  </section>

  <!-- Result Slots -->
  <section class="section">
    <h2>{$_('presentations.results.title')}</h2>
    <div class="result-slots">
      {#each [0, 1, 2, 3, 4] as i (i)}
        {@const file = $pptResults[i]}
        <button
          class="slot-btn"
          class:slot-filled={!!file}
          onclick={() => file && handleOpenSlot(file.path)}
          disabled={!file}
        >
          {file ? file.name : `— ${i + 1} —`}
        </button>
      {/each}
    </div>
  </section>

  {#if $useWebPresenter}
    <section class="section">
      <h2>Bible readings</h2>
      <div class="event-presenter-controls">
        <select
          class="event-select"
          value={selectedEventId}
          onchange={handleEventChange}
          disabled={eventsLoading || eventSummaries.length === 0}
          aria-label="Select event for Bible readings"
        >
          {#if eventSummaries.length === 0}
            <option value="">{eventsLoading ? 'Loading events...' : 'No events'}</option>
          {:else}
            {#each eventSummaries as event (event.id)}
              <option value={event.id}>{formatEventOption(event)}</option>
            {/each}
          {/if}
        </select>
        <div class="bible-actions">
          <button
            class="bible-btn"
            onclick={() => handlePresentBible('textus')}
            disabled={selectedEventLoading || !hasBibleVerses(textusReference)}
          >
            <span class="bible-btn-main">Textus</span>
            <span class="bible-btn-sub">{textusReference?.reference ?? 'No Textus'}</span>
          </button>
          <button
            class="bible-btn"
            onclick={() => handlePresentBible('leckio')}
            disabled={selectedEventLoading || !hasBibleVerses(leckioReference)}
          >
            <span class="bible-btn-main">Lekció</span>
            <span class="bible-btn-sub">{leckioReference?.reference ?? 'No Lekció'}</span>
          </button>
        </div>
      </div>
    </section>
  {/if}

  <!-- Presentation Status -->
  <section class="section">
    <h2>{$_('presentations.status.title')}</h2>
    {#if $useWebPresenter}
      <div class="status-grid">
        <div class="status-item">
          <span class="status-label">Loaded</span>
          <span class="status-value" class:active={$presenterState.loaded}>
            {$presenterState.loaded ? 'Yes' : 'No'}
          </span>
        </div>
        <div class="status-item">
          <span class="status-label">{$_('presentations.status.slide')}</span>
          <span class="status-value">
            {$presenterState.currentSlide || '—'} / {$presenterState.totalSlides || '—'}
          </span>
        </div>
        <div class="status-item">
          <span class="status-label">File</span>
          <span class="status-value file-name" title={$presenterState.filePath ?? ''}>
            {$presenterState.filePath ? $presenterState.filePath.split('/').pop() : '—'}
          </span>
        </div>
        <div class="status-item">
          <span class="status-label">Display</span>
          <span class="status-value" class:muted={$presenterState.muted}>
            {$presenterState.muted ? 'Muted' : 'Live'}
          </span>
        </div>
      </div>
      {#if $presenterState.loaded && $presenterState.renderMode === 'text'}
        <button
          class="btn-edit-slides"
          onclick={() => {
            slideEditorOpen = true;
          }}
        >
          Edit slide content
        </button>
      {/if}
    {:else}
      <div class="status-grid">
        <div class="status-item">
          <span class="status-label">{$_('presentations.status.slideshow')}</span>
          <span class="status-value" class:active={$keynoteStatus.slideshowActive}>
            {$keynoteStatus.slideshowActive
              ? $_('presentations.status.on')
              : $_('presentations.status.off')}
          </span>
        </div>
        <div class="status-item">
          <span class="status-label">{$_('presentations.status.slide')}</span>
          <span class="status-value">
            {$keynoteStatus.currentSlide ?? '—'} / {$keynoteStatus.totalSlides ?? '—'}
          </span>
        </div>
        <div class="status-item">
          <span class="status-label">{$_('presentations.status.document')}</span>
          <span class="status-value">{$keynoteStatus.documentName ?? '—'}</span>
        </div>
      </div>
    {/if}
  </section>

  <!-- Control Bar -->
  <section class="section">
    <h2>{$_('presentations.controls.title')}</h2>
    <div class="control-bar">
      <button class="ctrl-btn" onclick={handleFirst} aria-label={$_('presentations.controls.first')}
        >⏮</button
      >
      <button class="ctrl-btn" onclick={handlePrev} aria-label={$_('presentations.controls.prev')}
        >◀</button
      >
      <button class="ctrl-btn" onclick={handleNext} aria-label={$_('presentations.controls.next')}
        >▶</button
      >
      <button class="ctrl-btn" onclick={handleLast} aria-label={$_('presentations.controls.last')}
        >⏭</button
      >
      {#if !$useWebPresenter}
        <button
          class="ctrl-btn play-btn"
          onclick={handleStart}
          aria-label={$_('presentations.controls.play')}
          >▶ {$_('presentations.controls.play')}</button
        >
      {/if}
      <button
        class="ctrl-btn stop-btn"
        onclick={handleStopOrUnload}
        aria-label={$useWebPresenter ? 'Unload' : $_('presentations.controls.stop')}
        disabled={$useWebPresenter && !$presenterState.loaded}
      >
        {$useWebPresenter ? '⏹ Unload' : `⏹ ${$_('presentations.controls.stop')}`}
      </button>
      {#if $useWebPresenter}
        <button
          class="ctrl-btn mute-btn"
          class:mute-active={$presenterState.muted}
          onclick={handleToggleMute}
          aria-label={$presenterState.muted ? 'Unmute display' : 'Mute display'}
          disabled={!$presenterState.loaded}
        >
          {$presenterState.muted ? '⬛ Unmute' : '⬛ Mute'}
        </button>
      {/if}
      {#if !$useWebPresenter}
        <button
          class="ctrl-btn close-btn"
          onclick={handleCloseAll}
          aria-label={$_('presentations.controls.closeAll')}
          >✕ {$_('presentations.controls.closeAll')}</button
        >
      {/if}
    </div>
  </section>

  <!-- Web Presenter URL -->
  {#if $useWebPresenter}
    <section class="section">
      <h2>Presenter display</h2>
      <p class="note">
        Open this URL on a browser connected to the same network to display slides.
      </p>
      <div class="url-row">
        <code class="url-display">{buildPresenterUrl()}</code>
        <button class="btn-copy" onclick={copyPresenterUrl}>
          {copySuccess ? '✓ Copied' : 'Copy URL'}
        </button>
      </div>
    </section>

    <!-- Presenter Preview (iframe) -->
    <section class="section">
      <h2>Presenter preview</h2>
      <div class="iframe-wrapper">
        <iframe src={buildIframeUrl()} title="Presenter Display Preview" class="presenter-iframe"
        ></iframe>
      </div>
    </section>

    <!-- Connected clients -->
    <section class="section">
      <h2>Connected clients <span class="client-count">{$connectedClients.length}</span></h2>
      {#if $connectedClients.length === 0}
        <p class="note">No clients connected.</p>
      {:else}
        <ul class="client-list">
          {#each $connectedClients as client (client.id)}
            <li class="client-item">
              <div class="client-icon" aria-hidden="true">
                {#if client.label === 'Tauri App'}🖥{:else if client.label === 'Presenter Receiver'}📽{:else}🌐{/if}
              </div>
              <div class="client-info">
                <div class="client-top-row">
                  <span class="client-label">{client.label}</span>
                  {#if client.hostname}
                    <span class="client-hostname">{client.hostname}</span>
                  {/if}
                  <span class="client-browser">{parseBrowser(client.userAgent)}</span>
                </div>
                <div class="client-bottom-row">
                  <span
                    class="client-duration"
                    title={new Date(client.connectedAt).toLocaleString()}
                  >
                    Connected {formatDuration(client.connectedAt)}
                  </span>
                  {#if client.lastPongAt}
                    <span
                      class="client-pong"
                      title="Last pong: {new Date(client.lastPongAt).toLocaleTimeString()}"
                    >
                      Pong {formatDuration(client.lastPongAt)}
                    </span>
                  {/if}
                </div>
              </div>
              <div class="client-right">
                <span class="latency-badge {latencyClass(client.latencyMs)}">
                  {client.latencyMs !== null ? `${client.latencyMs} ms` : '—'}
                </span>
                <button
                  class="btn-ping"
                  onclick={() => pingClient(client.id)}
                  disabled={pingingClient === client.id}
                  aria-label="Ping {client.label}"
                >
                  {pingingClient === client.id ? '…' : 'Ping'}
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

{#if slideEditorOpen && $useWebPresenter && $presenterState.renderMode === 'text'}
  <SlideEditorModal
    slides={$presenterState.slides}
    onclose={() => {
      slideEditorOpen = false;
    }}
  />
{/if}

<style>
  .presentations-page {
    padding: 1rem 0;
  }

  h1 {
    font-size: 1.5rem;
    font-weight: 600;
    margin-bottom: 1.5rem;
  }

  .section {
    margin-bottom: 2rem;
  }

  h2 {
    font-size: 1.1rem;
    font-weight: 600;
    margin-bottom: 0.75rem;
    color: var(--text-primary);
  }

  .folder-list {
    list-style: none;
    padding: 0;
    margin: 0 0 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .folder-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    background: var(--content-bg);
    border-radius: 0.375rem;
  }

  .folder-name {
    font-weight: 500;
    min-width: 120px;
  }

  .folder-path {
    flex: 1;
    color: var(--text-secondary);
    font-size: 0.875rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .folder-item button {
    padding: 0.25rem 0.5rem;
    background: var(--status-err-dot);
    color: white;
    border: none;
    border-radius: 0.25rem;
    cursor: pointer;
    font-size: 0.75rem;
  }

  .btn-primary {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .folder-error {
    margin: 0.5rem 0 0;
    font-size: 0.8125rem;
    color: var(--status-err-text);
  }

  .btn-edit-slides {
    margin-top: 0.75rem;
    padding: 0.5rem 1rem;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .btn-edit-slides:hover {
    background: color-mix(in oklch, var(--accent) 12%, transparent);
  }

  .filter-display {
    padding: 0.5rem 0.75rem;
    background: var(--text-primary);
    color: white;
    border-radius: 0.375rem;
    font-size: 1.5rem;
    font-family: monospace;
    min-height: 2.5rem;
    margin-bottom: 0.75rem;
    letter-spacing: 0.1em;
  }

  .digit-pad {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.5rem;
    max-width: 280px;
  }

  .digit-btn {
    padding: 0.75rem;
    font-size: 1.25rem;
    font-weight: 600;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
  }

  .clear-btn {
    background: var(--status-warn-dot);
  }

  .result-slots {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .slot-btn {
    padding: 0.75rem 1rem;
    background: var(--text-secondary);
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    text-align: left;
    font-size: 0.875rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slot-btn.slot-filled {
    background: var(--status-ok-dot);
  }

  .slot-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .event-presenter-controls {
    display: flex;
    align-items: stretch;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .event-select {
    flex: 1 1 22rem;
    min-width: 16rem;
    padding: 0.6rem 0.75rem;
    background: var(--content-bg);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
  }

  .event-select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .bible-actions {
    display: flex;
    gap: 0.5rem;
    flex: 0 0 auto;
  }

  .bible-btn {
    width: 9.5rem;
    min-height: 3rem;
    padding: 0.45rem 0.65rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    text-align: left;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.1rem;
  }

  .bible-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .bible-btn-main {
    font-size: 0.85rem;
    font-weight: 700;
    line-height: 1.1;
  }

  .bible-btn-sub {
    font-size: 0.72rem;
    line-height: 1.2;
    opacity: 0.9;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1rem;
  }

  .status-item {
    padding: 0.75rem;
    background: var(--content-bg);
    border-radius: 0.375rem;
  }

  .status-label {
    display: block;
    font-size: 0.75rem;
    color: var(--text-secondary);
    margin-bottom: 0.25rem;
  }

  .status-value {
    font-weight: 600;
    font-size: 0.875rem;
  }

  .status-value.active {
    color: var(--status-ok-dot);
  }

  .status-value.file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }

  .control-bar {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .ctrl-btn {
    padding: 0.5rem 0.75rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .play-btn {
    background: var(--status-ok-dot);
  }

  .stop-btn {
    background: var(--status-err-dot);
  }

  .close-btn {
    background: var(--status-err-dot);
  }

  .mute-btn {
    background: var(--text-secondary);
  }

  .mute-btn.mute-active {
    background: var(--status-warn-dot);
  }

  .mute-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .status-value.muted {
    color: var(--status-warn-dot);
  }

  .note {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0 0 0.75rem;
  }

  .url-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .url-display {
    flex: 1;
    padding: 0.5rem 0.75rem;
    background: var(--content-bg);
    border-radius: 0.375rem;
    font-family: monospace;
    font-size: 0.8rem;
    word-break: break-all;
    min-width: 0;
  }

  .btn-copy {
    padding: 0.5rem 0.75rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
    white-space: nowrap;
  }

  .iframe-wrapper {
    aspect-ratio: 16 / 9;
    width: 100%;
    max-width: 640px;
    background: #000;
    border-radius: 0.375rem;
    overflow: hidden;
    border: 1px solid var(--border);
  }

  .presenter-iframe {
    width: 100%;
    height: 100%;
    border: none;
    display: block;
  }

  .client-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.25rem;
    height: 1.25rem;
    padding: 0 0.3rem;
    background: var(--accent);
    color: white;
    border-radius: 9999px;
    font-size: 0.7rem;
    font-weight: 700;
    vertical-align: middle;
    margin-left: 0.4rem;
  }

  .client-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .client-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 0.75rem;
    background: var(--content-bg);
    border-radius: 0.375rem;
  }

  .client-icon {
    font-size: 1.25rem;
    flex-shrink: 0;
    line-height: 1;
  }

  .client-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
  }

  .client-top-row {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
  }

  .client-label {
    font-weight: 600;
    font-size: 0.875rem;
  }

  .client-browser {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .client-hostname {
    font-size: 0.75rem;
    font-family: monospace;
    color: var(--text-secondary);
    background: var(--surface-secondary, oklch(0.92 0 0));
    padding: 0.05rem 0.35rem;
    border-radius: 0.25rem;
  }

  .client-bottom-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .client-duration,
  .client-pong {
    font-size: 0.72rem;
    color: var(--text-secondary);
  }

  .client-pong::before {
    content: '·';
    margin-right: 0.4rem;
  }

  .client-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.35rem;
    flex-shrink: 0;
  }

  .latency-badge {
    font-size: 0.72rem;
    font-weight: 600;
    padding: 0.15rem 0.45rem;
    border-radius: 9999px;
    white-space: nowrap;
  }

  .latency-good {
    background: color-mix(in oklch, var(--status-ok-dot) 20%, transparent);
    color: var(--status-ok-dot);
  }

  .latency-ok {
    background: color-mix(in oklch, var(--status-warn-dot) 20%, transparent);
    color: var(--status-warn-dot);
  }

  .latency-slow {
    background: color-mix(in oklch, var(--status-err-dot) 20%, transparent);
    color: var(--status-err-dot);
  }

  .latency-unknown {
    background: color-mix(in oklch, var(--text-secondary) 15%, transparent);
    color: var(--text-secondary);
  }

  .btn-ping {
    padding: 0.25rem 0.6rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.25rem;
    cursor: pointer;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .btn-ping:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  @media (max-width: 640px) {
    .event-select {
      min-width: 0;
      flex-basis: 100%;
    }

    .bible-actions {
      width: 100%;
    }

    .bible-btn {
      flex: 1 1 0;
      width: auto;
      min-width: 0;
    }
  }
</style>
