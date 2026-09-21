<script lang="ts">
  import { openExternal } from '@metocast/core-client';
  import { youtubeStatus } from '$lib/stores/connectors.js';
  import { fetchYouTubeContent, type ChannelVideoItem } from '@metocast/core-client';
  import { ApiError } from '@metocast/core-client';
  import { _ } from 'svelte-i18n';

  type PlatformId = 'youtube';
  type ContentTab = 'videos' | 'live';

  const platforms: { id: PlatformId; label: string }[] = [{ id: 'youtube', label: 'YouTube' }];

  let selectedPlatform = $state<PlatformId>('youtube');
  let selectedTab = $state<ContentTab>('videos');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let isAuthError = $state(false);
  let broadcasts = $state<ChannelVideoItem[]>([]);
  let videos = $state<ChannelVideoItem[]>([]);
  let hasFetched = $state(false);

  function isYouTubeConnected(): boolean {
    return $youtubeStatus === 'connected';
  }

  let visibleItems = $derived(selectedTab === 'live' ? broadcasts : videos);

  async function load() {
    if (!isYouTubeConnected()) return;
    loading = true;
    error = null;
    isAuthError = false;
    hasFetched = true;
    try {
      const content = await fetchYouTubeContent();
      broadcasts = content.liveBroadcasts;
      videos = content.videos;
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        isAuthError = true;
      }
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function retry() {
    hasFetched = false;
    isAuthError = false;
    await load();
  }

  const studioUrl = (item: ChannelVideoItem) =>
    `https://studio.youtube.com/video/${item.id}/${
      item.liveStatus === 'live' || item.liveStatus === 'upcoming' ? 'livestreaming' : 'edit'
    }`;

  async function openVideo(url: string) {
    try {
      await openExternal(url);
    } catch {
      window.open(url, '_blank');
    }
  }

  function formatViews(count: number | null | undefined): string {
    if (count == null) return '';
    if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M views`;
    if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K views`;
    return `${count} view${count !== 1 ? 's' : ''}`;
  }

  function formatRelativeDate(dateStr: string | null | undefined): string {
    if (!dateStr) return '';
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / 86_400_000);
    if (diffDays < 0) return formatAbsoluteDate(dateStr);
    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays} days ago`;
    if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
    if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
    return `${Math.floor(diffDays / 365)} years ago`;
  }

  function formatAbsoluteDate(dateStr: string | null | undefined): string {
    if (!dateStr) return '';
    return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(
      new Date(dateStr),
    );
  }

  $effect(() => {
    if ($youtubeStatus === 'connected' && !hasFetched) {
      load();
    }
  });
</script>

<div class="page">
  <div class="page-header">
    <h1 class="page-title">{$_('liveEvents.title')}</h1>
  </div>

  <!-- Platform tabs -->
  <div class="platform-tabs" role="tablist" aria-label="Platform">
    {#each platforms as p (p.id)}
      <button
        role="tab"
        aria-selected={selectedPlatform === p.id}
        class="platform-tab"
        class:active={selectedPlatform === p.id}
        onclick={() => (selectedPlatform = p.id)}
      >
        {#if p.id === 'youtube'}
          <svg class="platform-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path
              d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z"
            />
          </svg>
        {/if}
        {p.label}
      </button>
    {/each}
  </div>

  {#if selectedPlatform === 'youtube'}
    {#if !isYouTubeConnected()}
      <div class="not-connected">
        <svg
          class="not-connected-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          aria-hidden="true"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M13.19 8.688a4.5 4.5 0 0 1 1.242 7.244l-4.5 4.5a4.5 4.5 0 0 1-6.364-6.364l1.757-1.757m13.35-.622 1.757-1.757a4.5 4.5 0 0 0-6.364-6.364l-4.5 4.5a4.5 4.5 0 0 0 1.242 7.244"
          />
        </svg>
        <p class="not-connected-title">{$_('liveEvents.notConnected')}</p>
        <p class="not-connected-desc">{$_('liveEvents.connectFirst')}</p>
        <a href="/settings" class="settings-link">Go to Settings</a>
      </div>
    {:else}
      <!-- Content tabs (Videos | Live) -->
      <div class="content-tabs" role="tablist" aria-label="Content type">
        <button
          role="tab"
          aria-selected={selectedTab === 'videos'}
          class="content-tab"
          class:active={selectedTab === 'videos'}
          onclick={() => (selectedTab = 'videos')}
        >
          {$_('liveEvents.videos')}
          {#if !loading && videos.length > 0}
            <span class="tab-count">{videos.length}</span>
          {/if}
        </button>
        <button
          role="tab"
          aria-selected={selectedTab === 'live'}
          class="content-tab"
          class:active={selectedTab === 'live'}
          onclick={() => (selectedTab = 'live')}
        >
          {$_('liveEvents.broadcasts')}
          {#if !loading && broadcasts.length > 0}
            <span class="tab-count">{broadcasts.length}</span>
          {/if}
        </button>
      </div>

      <!-- Loading -->
      {#if loading}
        <div class="video-grid">
          {#each { length: 8 } as _, i (i)}
            <div class="video-card skeleton-card" aria-hidden="true">
              <div class="thumbnail-wrap skeleton"></div>
              <div class="card-info">
                <div class="skeleton skeleton-title"></div>
                <div class="skeleton skeleton-meta"></div>
              </div>
            </div>
          {/each}
        </div>

        <!-- Error -->
      {:else if error}
        <div class="error-state">
          <p class="error-msg">
            {isAuthError ? $_('liveEvents.sessionExpired') : $_('liveEvents.error')}
          </p>
          {#if !isAuthError}
            <p class="error-detail">{error}</p>
          {/if}
          {#if isAuthError}
            <a href="/settings" class="pill-btn">{$_('liveEvents.reLogin')}</a>
          {:else}
            <button class="pill-btn" onclick={retry}>{$_('liveEvents.retry')}</button>
          {/if}
        </div>

        <!-- Empty -->
      {:else if visibleItems.length === 0}
        <div class="empty-state">
          <p class="empty-msg">{$_('liveEvents.noContent')}</p>
        </div>

        <!-- Grid -->
      {:else}
        <div class="video-grid">
          {#each visibleItems as item (item.id)}
            <button class="video-card" onclick={() => openVideo(studioUrl(item))}>
              <div class="thumbnail-wrap">
                {#if item.thumbnailUrl}
                  <img
                    class="thumbnail"
                    src={item.thumbnailUrl}
                    alt={item.title}
                    loading="lazy"
                    onerror={(e) => e.currentTarget.remove()}
                  />
                {:else}
                  <div class="thumbnail-placeholder" aria-hidden="true"></div>
                {/if}

                {#if item.liveStatus === 'live'}
                  <span class="badge badge-live">{$_('liveEvents.live')}</span>
                {:else if item.liveStatus === 'upcoming'}
                  <span class="badge badge-upcoming">{$_('liveEvents.upcoming')}</span>
                {/if}

                {#if item.duration && item.liveStatus !== 'live' && item.liveStatus !== 'upcoming'}
                  <span class="duration-badge">{item.duration}</span>
                {/if}
              </div>

              <div class="card-info">
                <p class="video-title" title={item.title}>{item.title}</p>
                <p class="video-meta">
                  {#if item.viewCount != null}
                    <span>{formatViews(item.viewCount)}</span>
                  {/if}
                  {#if selectedTab === 'live' && item.liveStatus === 'upcoming' && item.scheduledStartTime}
                    {#if item.viewCount != null}<span class="meta-sep">·</span>{/if}
                    <span>{formatAbsoluteDate(item.scheduledStartTime)}</span>
                  {:else if item.publishedAt}
                    {#if item.viewCount != null}<span class="meta-sep">·</span>{/if}
                    <span>{formatRelativeDate(item.publishedAt)}</span>
                  {/if}
                </p>
                {#if item.privacyStatus !== 'public'}
                  <span class="privacy-badge privacy-{item.privacyStatus}">
                    {#if item.privacyStatus === 'private'}
                      <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"
                        ><path
                          d="M8 1a3.5 3.5 0 0 0-3.5 3.5V6H3a1 1 0 0 0-1 1v7a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V7a1 1 0 0 0-1-1h-1.5V4.5A3.5 3.5 0 0 0 8 1zm2.5 5H5.5V4.5a2.5 2.5 0 0 1 5 0V6z"
                        /></svg
                      >
                      Private
                    {:else}
                      <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"
                        ><path
                          d="M3.5 8a.5.5 0 0 1 .5-.5h8a.5.5 0 0 1 0 1H4a.5.5 0 0 1-.5-.5zm-2-3a.5.5 0 0 1 .5-.5h12a.5.5 0 0 1 0 1H2a.5.5 0 0 1-.5-.5zm5 6a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 0 1H7a.5.5 0 0 1-.5-.5z"
                        /></svg
                      >
                      Unlisted
                    {/if}
                  </span>
                {/if}
              </div>
            </button>
          {/each}
        </div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  .page-header {
    display: flex;
    align-items: center;
  }

  .page-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0;
  }

  /* ── Platform tabs ── */
  .platform-tabs {
    display: flex;
    gap: 0.25rem;
    border-bottom: 2px solid var(--border);
  }

  .platform-tab {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1.25rem;
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--text-secondary);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    cursor: pointer;
    transition:
      color 0.15s,
      border-color 0.15s;
  }

  .platform-tab:hover {
    color: var(--text-primary);
  }

  .platform-tab.active {
    color: var(--text-primary);
    border-bottom-color: var(--text-primary);
  }

  .platform-icon {
    width: 1.25rem;
    height: 1.25rem;
    color: #ff0000;
    flex-shrink: 0;
  }

  /* ── Content tabs (Videos | Live) ── */
  .content-tabs {
    display: flex;
    gap: 0.5rem;
  }

  .content-tab {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.375rem 1rem;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-secondary);
    background: var(--content-bg);
    border: none;
    border-radius: 9999px;
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s;
  }

  .content-tab:hover {
    background: var(--nav-item-hover);
    color: var(--text-primary);
  }

  .content-tab.active {
    background: var(--text-primary);
    color: white;
  }

  .tab-count {
    font-size: 0.75rem;
    font-weight: 600;
    opacity: 0.7;
  }

  /* ── Not connected ── */
  .not-connected {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    padding: 4rem 2rem;
    text-align: center;
    color: var(--text-secondary);
  }

  .not-connected-icon {
    width: 3rem;
    height: 3rem;
    color: var(--text-tertiary);
  }

  .not-connected-title {
    font-size: 1.125rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .not-connected-desc {
    font-size: 0.875rem;
    max-width: 24rem;
    margin: 0;
  }

  .settings-link {
    display: inline-block;
    margin-top: 0.5rem;
    padding: 0.5rem 1.25rem;
    background: var(--text-primary);
    color: white;
    border-radius: 9999px;
    font-size: 0.875rem;
    font-weight: 600;
    text-decoration: none;
    transition: filter 0.15s;
  }

  .settings-link:hover {
    filter: brightness(0.8);
  }

  /* ── Error ── */
  .error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 3rem 2rem;
    text-align: center;
  }

  .error-msg {
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .error-detail {
    font-size: 0.8125rem;
    color: var(--text-secondary);
    max-width: 32rem;
    margin: 0;
  }

  .pill-btn {
    margin-top: 0.75rem;
    padding: 0.5rem 1.25rem;
    background: var(--text-primary);
    color: white;
    border: none;
    border-radius: 9999px;
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
    transition: filter 0.15s;
  }

  .pill-btn:hover {
    filter: brightness(0.8);
  }

  /* ── Empty ── */
  .empty-state {
    padding: 3rem 0;
  }

  .empty-msg {
    color: var(--text-secondary);
    font-size: 0.875rem;
    margin: 0;
  }

  /* ── Video grid ── */
  .video-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 1rem 0.75rem;
  }

  @media (max-width: 1100px) {
    .video-grid {
      grid-template-columns: repeat(3, 1fr);
    }
  }

  @media (max-width: 780px) {
    .video-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 480px) {
    .video-grid {
      grid-template-columns: 1fr;
    }
  }

  /* ── Video card ── */
  .video-card {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    cursor: pointer;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    border-radius: 0.5rem;
    transition: transform 0.1s;
  }

  .video-card:hover {
    transform: translateY(-2px);
  }

  .video-card:focus-visible {
    outline: 2px solid #065fd4;
    outline-offset: 2px;
    border-radius: 0.5rem;
  }

  .skeleton-card {
    cursor: default;
  }

  .skeleton-card:hover {
    transform: none;
  }

  /* ── Thumbnail ── */
  .thumbnail-wrap {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: 0.5rem;
    overflow: hidden;
    background: var(--border);
  }

  .thumbnail {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .thumbnail-placeholder {
    width: 100%;
    height: 100%;
    background: var(--border);
  }

  /* ── Badges ── */
  .badge {
    position: absolute;
    top: 0.375rem;
    left: 0.375rem;
    padding: 0.125rem 0.375rem;
    border-radius: 0.25rem;
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.03em;
    color: #fff;
  }

  .badge-live {
    background: #ff0000;
  }
  .badge-upcoming {
    background: var(--text-primary);
  }

  .duration-badge {
    position: absolute;
    bottom: 0.375rem;
    right: 0.375rem;
    padding: 0.125rem 0.3125rem;
    background: rgba(0, 0, 0, 0.8);
    color: #fff;
    border-radius: 0.25rem;
    font-size: 0.6875rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
  }

  /* ── Card info ── */
  .card-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0 0.125rem;
  }

  .video-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .video-meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: var(--text-secondary);
    margin: 0;
  }

  .meta-sep {
    color: var(--text-tertiary);
  }

  /* ── Privacy badge ── */
  .privacy-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    margin-top: 0.2rem;
    padding: 0.1rem 0.375rem;
    border-radius: 0.25rem;
    font-size: 0.6875rem;
    font-weight: 600;
    width: fit-content;
  }

  .privacy-badge svg {
    width: 0.625rem;
    height: 0.625rem;
    flex-shrink: 0;
  }

  .privacy-private {
    background: var(--status-err-bg);
    color: var(--status-err-text);
  }

  .privacy-unlisted {
    background: var(--status-warn-bg);
    color: var(--status-warn-text);
  }

  /* ── Skeleton ── */
  .skeleton {
    background: linear-gradient(90deg, var(--border) 25%, var(--content-bg) 50%, var(--border) 75%);
    background-size: 200% 100%;
    animation: shimmer 1.5s infinite;
    border-radius: 0.25rem;
  }

  .skeleton-title {
    height: 0.875rem;
    width: 90%;
    margin-top: 0.5rem;
  }

  .skeleton-meta {
    height: 0.75rem;
    width: 60%;
    margin-top: 0.25rem;
  }

  @keyframes shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }
</style>
