<script lang="ts">
  import '@metocast/design-system/tokens.css';
  import '@metocast/design-system/fonts.css';
  import '@metocast/design-system/base.css';
  import '$lib/i18n';
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { _ } from 'svelte-i18n';
  import { enableWindowGlass, connectWs, getAppMode, onConnectLink } from '@metocast/core-client';
  import type { WsStatus, HostMode, ConnectLink } from '@metocast/core-client';
  import {
    ToastOverlay,
    Toast,
    Glyph,
    NotificationCentre,
    Dialog,
    Button,
  } from '@metocast/design-system';
  import { initCore } from '$lib/core';
  import { initScheme } from '$lib/scheme.svelte';
  import { handleWs, registerLive } from '$lib/live.svelte';
  import {
    railItems,
    dismissRail,
    dismiss,
    allNotifications,
    activeIssues,
    historyNotifications,
    activeIssueCount,
    isCentreOpen,
    closeCentre,
    clearAll,
    type Notification,
  } from '$lib/notifications.svelte';
  import Nav from '$lib/Nav.svelte';
  import Setup from '$lib/Setup.svelte';

  let { children } = $props();

  let status = $state<WsStatus>('connecting');
  let phase = $state<'loading' | 'setup' | 'app'>('loading');
  let pendingLink = $state<ConnectLink | null>(null);
  let overrideLink = $state<ConnectLink | null>(null);
  const isPresenter = $derived(page.url.pathname === '/presenter');
  const notificationPreview =
    import.meta.env.DEV && import.meta.env.VITE_SANCTUM_UX_PREVIEW === 'connector-notifications';

  function receiveConnectLink(link: ConnectLink) {
    if (phase === 'setup') pendingLink = link;
    else overrideLink = link;
  }

  function acceptOverride() {
    pendingLink = overrideLink;
    overrideLink = null;
    phase = 'setup';
  }

  function centreTitle(): string {
    const active = activeIssueCount();
    if (active > 0)
      return $_(active === 1 ? 'notif.activeCountOne' : 'notif.activeCount', {
        values: { n: active },
      });
    const total = allNotifications().length;
    return total > 0 ? $_('notif.count', { values: { n: total } }) : $_('notif.allClear');
  }

  onMount(async () => {
    if (isPresenter) return;

    initScheme();
    enableWindowGlass()
      .then(() => document.body.classList.add('glass'))
      .catch(() => {});

    if (notificationPreview) {
      status = 'connected';
      phase = 'app';
      const { seedConnectorNotificationPreview } =
        await import('$lib/dev/connector-notification-preview');
      seedConnectorNotificationPreview();
      return;
    }

    let mode: HostMode | null = 'server';
    try {
      mode = await getAppMode();
    } catch {
      mode = 'server';
    }
    if (mode !== 'server' && mode !== 'client') {
      phase = 'setup';
    } else {
      try {
        await initCore();
      } catch {
        /* no desktop host / not configured — stay on defaults */
      }
      connectWs({
        onStatus: (s) => (status = s),
        onOpen: () => registerLive(),
        onMessage: (msg) => handleWs(msg),
      });
      phase = 'app';
    }
    onConnectLink(receiveConnectLink);
  });
</script>

{#if isPresenter}
  {@render children()}
{:else}
  <div class="titlebar" data-tauri-drag-region aria-hidden="true"></div>

{#if phase === 'setup'}
  <Setup connect={pendingLink} />
{:else if phase === 'app'}
  {#snippet connectorGlyph(t: Notification)}
    {#if t.brand}
      <Glyph size={34} label={t.source}>
        {#snippet mark()}<svg
            viewBox={t.brandBox ?? '0 0 24 24'}
            width="19"
            height="19"
            fill="currentColor"
            aria-hidden="true"><path d={t.brand} /></svg
          >{/snippet}
      </Glyph>
    {:else}
      <Glyph char={t.source.charAt(0)} size={34} />
    {/if}
  {/snippet}

  <Nav />

  {#if status !== 'connected'}
    <div class="conn" role="status">
      {status === 'connecting' ? $_('connection.connecting') : $_('connection.lost')}
    </div>
  {/if}

  <main class="content">
    {@render children()}
  </main>

  <div class="toast-host">
    <ToastOverlay label={$_('toast.region')} priority="assertive">
      {#each railItems() as t (t.id)}
        <Toast
          kind={t.kind}
          source={t.source}
          title={t.title}
          body={t.body}
          mono={t.mono}
          tone={t.tier}
          state={t.state}
          actions={(t.actions ?? []).map((a) => ({
            label: a.label,
            primary: a.primary,
            onclick: a.run,
          }))}
          remediation={t.remediation}
          whyLabel={$_('notif.showSteps')}
          hideWhyLabel={$_('notif.hideSteps')}
          dismissLabel={$_('toast.minimize')}
          ondismiss={() => dismissRail(t.id)}
        >
          {#snippet mark()}{@render connectorGlyph(t)}{/snippet}
        </Toast>
      {/each}
    </ToastOverlay>
  </div>

  <NotificationCentre
    open={isCentreOpen()}
    eyebrow={$_('notif.eyebrow')}
    title={centreTitle()}
    empty={allNotifications().length === 0}
    emptyTitle={$_('notif.emptyTitle')}
    emptyHint={$_('notif.emptyHint')}
    clearLabel={$_('notif.clearHistory')}
    onclear={historyNotifications().length > 0 ? clearAll : undefined}
    onclose={closeCentre}
  >
    {#if activeIssues().length > 0}
      <section class="notification-section" aria-labelledby="active-issues-heading">
        <h3 id="active-issues-heading">
          <span>{$_('notif.activeIssues')}</span>
          <small>{activeIssues().length}</small>
        </h3>
        <div class="notification-stack">
          {#each activeIssues() as t (t.id)}
            <Toast
              kind={t.kind}
              source={t.source}
              title={t.title}
              body={t.body}
              mono={t.mono}
              tone={t.tier}
              state={t.state}
              actions={(t.actions ?? []).map((a) => ({
                label: a.label,
                primary: a.primary,
                onclick: a.run,
              }))}
              remediation={t.remediation}
              whyLabel={$_('notif.showSteps')}
              hideWhyLabel={$_('notif.hideSteps')}
            >
              {#snippet mark()}{@render connectorGlyph(t)}{/snippet}
              {#snippet detail()}
                {#if t.group?.length}
                  <ul class="group">
                    {#each t.group as g (g.source)}<li>{g.source} — {g.label}</li>{/each}
                  </ul>
                {/if}
              {/snippet}
            </Toast>
          {/each}
        </div>
      </section>
    {/if}

    {#if historyNotifications().length > 0}
      <section class="notification-section" aria-labelledby="recent-notifications-heading">
        <h3 id="recent-notifications-heading">
          <span>{$_('notif.recentNotifications')}</span>
          <small>{historyNotifications().length}</small>
        </h3>
        <div class="notification-stack">
          {#each historyNotifications() as t (t.id)}
            <Toast
              kind={t.kind}
              source={t.source}
              title={t.title}
              body={t.body}
              mono={t.mono}
              tone={t.tier}
              state={t.state}
              actions={(t.actions ?? []).map((a) => ({
                label: a.label,
                primary: a.primary,
                onclick: a.run,
              }))}
              remediation={t.remediation}
              whyLabel={$_('notif.showSteps')}
              hideWhyLabel={$_('notif.hideSteps')}
              dismissLabel={$_('toast.dismiss')}
              ondismiss={() => dismiss(t.id)}
            >
              {#snippet mark()}{@render connectorGlyph(t)}{/snippet}
              {#snippet detail()}
                {#if t.group?.length}
                  <ul class="group">
                    {#each t.group as g (g.source)}<li>{g.source} — {g.label}</li>{/each}
                  </ul>
                {/if}
              {/snippet}
            </Toast>
          {/each}
        </div>
      </section>
    {/if}
  </NotificationCentre>
{/if}

<Dialog
  open={overrideLink !== null}
  eyebrow={$_('setup.override.eyebrow')}
  title={$_('setup.override.title')}
  onclose={() => (overrideLink = null)}
>
  {$_('setup.override.body', { values: { url: overrideLink?.url ?? '' } })}
  {#snippet footer()}
    <Button variant="secondary" compact onclick={() => (overrideLink = null)}>
      {$_('setup.override.keep')}
    </Button>
    <Button variant="primary" compact onclick={acceptOverride}
      >{$_('setup.override.replace')}</Button
    >
  {/snippet}
</Dialog>

  <div id="overlays"></div>
{/if}

<style>
  .titlebar {
    position: fixed;
    inset: 0 0 auto 0;
    height: max(44px, env(safe-area-inset-top, 44px));
    z-index: 100;
  }

  /* The host webview is transparent, so a page shorter than the screen would let
     the window show through below the content. */
  :global(body) {
    min-height: 100dvh;
  }

  :global(body.glass) {
    background: color-mix(in srgb, var(--surface-outside) 82%, transparent);
  }

  /* A quiet indicator in the top-right, not a full-width bar. */
  .conn {
    position: fixed;
    top: 52px;
    right: 16px;
    z-index: 60;
    padding: 4px 10px;
    font-family: var(--font-label);
    font-size: 10px;
    letter-spacing: 0.6px;
    background: var(--surface-raised);
    color: var(--text-muted);
    border: 1px solid color-mix(in srgb, var(--text-primary) 14%, transparent);
  }

  /* Not a fixed scroll pane: on iOS that becomes the containing block for the
     fixed children inside it, clipping every page-level Sheet at the nav. */
  .content {
    padding-top: max(44px, env(safe-area-inset-top, 44px));
    padding-bottom: calc(57px + max(14px, env(safe-area-inset-bottom, 14px)));
  }

  .group {
    margin: 8px 0 0;
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 1.5;
    list-style: none;
    padding-left: 0;
  }

  .notification-section {
    display: flex;
    flex-direction: column;
    gap: var(--ui-stack);
  }

  .notification-section + .notification-section {
    margin-top: var(--ui-stack-loose);
  }

  .notification-section h3 {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--ui-stack);
    margin: 0;
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    font-weight: var(--type-label-weight);
    letter-spacing: var(--type-label-track);
    text-transform: var(--type-label-transform);
    color: var(--text-muted);
  }

  .notification-section h3 small {
    color: var(--text-faint);
    font: inherit;
  }

  .notification-stack {
    display: flex;
    flex-direction: column;
    gap: var(--c-notification-centre-gap);
  }

  /* On tablet/desktop the toast rail settles to the right at a capped width, rather
     than stretching the full window as the DS overlay does by default. */
  @media (min-width: 760px) {
    .toast-host :global(.overlay) {
      left: auto;
      /* A definite width, not shrink-to-fit: every toast in the rail is the same
         width whether or not it carries a body line. */
      width: 380px;
    }
  }

  @media (min-width: 980px) {
    .content {
      padding-top: 44px;
      padding-bottom: 0;
      margin-left: 226px;
    }
  }

  /* The host draws the nav itself: no in-page bar or rail to leave room for —
     only the toolbar strip the macOS window grew at the top. iOS keeps the
     bottom inset, which the native tab bar occupies instead. */
  :global(body.native-nav-top) .content {
    padding-top: 64px;
    padding-bottom: 0;
    margin-left: 0;
  }

  :global(body.native-nav-bottom) .content {
    margin-left: 0;
    /* The native bar's own height plus the gap it keeps above the home indicator. */
    padding-bottom: calc(66px + env(safe-area-inset-bottom, 0px));
  }
</style>
