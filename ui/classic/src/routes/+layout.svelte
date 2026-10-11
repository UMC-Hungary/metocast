<script lang="ts">
  import { resolve } from '$app/paths';
  import '../app.css';
  import '$lib/configure-core-client';
  import { Toaster } from 'svelte-sonner';
  import '$lib/i18n';
  import { _ } from 'svelte-i18n';
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { initSystemAppearance, systemTheme } from '$lib/stores/system-appearance.js';
  import ConnectorInit from '$lib/components/layout/ConnectorInit.svelte';
  import ReLoginHandler from '$lib/components/layout/ReLoginHandler.svelte';
  import UpdateChecker from '$lib/components/layout/UpdateChecker.svelte';
  import NavConnectors from '$lib/components/layout/NavConnectors.svelte';
  import NavErrorBadge from '$lib/components/layout/NavErrorBadge.svelte';
  let { children } = $props();

  onMount(async () => {
    await initSystemAppearance();
  });

  $effect(() => {
    document.documentElement.setAttribute('data-theme', $systemTheme);
  });

  function isActive(href: string): boolean {
    if (href === '/') {
      return $page.url.pathname === '/';
    }
    return $page.url.pathname.startsWith(href);
  }
</script>

{#if $page.url.pathname.startsWith('/caption') || $page.url.pathname.startsWith('/presenter') || $page.url.pathname.startsWith('/transcript')}
  {@render children()}
{:else}
  <Toaster richColors position="top-right" closeButton />
  <ConnectorInit />
  <ReLoginHandler />
  <UpdateChecker />

  <div class="app-shell">
    <aside class="sidebar">
      <nav class="sidebar-nav" data-tauri-drag-region>
        <a href={resolve('/')} class="nav-item" class:active={isActive('/')}
          >{$_('nav.dashboard')}</a
        >
        <a href={resolve('/events')} class="nav-item" class:active={isActive('/events')}
          >{$_('nav.events')}</a
        >
        <a href={resolve('/live-events')} class="nav-item" class:active={isActive('/live-events')}
          >{$_('nav.liveEvents')}</a
        >
        <a
          href={resolve('/presentations')}
          class="nav-item"
          class:active={isActive('/presentations')}>{$_('nav.presentations')}</a
        >
        <a href={resolve('/obs-caption')} class="nav-item" class:active={isActive('/obs-caption')}
          >{$_('nav.obsCaption')}</a
        >
        <a href={resolve('/obs-devices')} class="nav-item" class:active={isActive('/obs-devices')}
          >{$_('nav.obsDevices')}</a
        >
        <a href={resolve('/connect')} class="nav-item" class:active={isActive('/connect')}
          >{$_('nav.connect')}</a
        >
        <a href={resolve('/queues')} class="nav-item" class:active={isActive('/queues')}
          >{$_('nav.queues')}</a
        >
        <a href={resolve('/logs')} class="nav-item" class:active={isActive('/logs')}
          >{$_('nav.logs')}</a
        >
        <a href={resolve('/settings')} class="nav-item" class:active={isActive('/settings')}
          >{$_('nav.settings')}</a
        >
      </nav>
      <div class="sidebar-footer">
        <NavErrorBadge />
        <NavConnectors />
      </div>
    </aside>
    <main class="content-pane">
      {@render children()}
    </main>
  </div>
{/if}

<style>
  .app-shell {
    position: relative;
    display: flex;
    height: 100vh;
    overflow: hidden;
  }

  .sidebar {
    width: var(--sidebar-width);
    flex-shrink: 0;
    background: var(--glass-sidebar-bg);
    /* Separator line + soft shadow cast onto the content pane */
    border-right: 1px solid var(--glass-border);
    box-shadow: 4px 0 24px rgba(0, 0, 0, 0.12);
    display: flex;
    flex-direction: column;
    /* Keep sidebar above content so its shadow is visible */
    position: relative;
    z-index: 1;
  }

  .sidebar-nav {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 0.5rem 0;
    overflow-y: auto;
  }

  .sidebar-footer {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem 0.5rem;
    border-top: 1px solid var(--glass-border);
  }

  .content-pane {
    flex: 1;
    overflow-y: auto;
    background: var(--content-bg);
    padding: 1rem 1.5rem 1.5rem;
  }

  .nav-item {
    display: flex;
    align-items: center;
    padding: 8px 16px;
    border-radius: 8px;
    margin: 2px 8px;
    color: var(--nav-item-text);
    font-weight: 500;
    font-size: 14px;
    text-decoration: none;
    transition: background 0.1s;
  }

  .nav-item:hover {
    background: var(--nav-item-hover);
  }

  .nav-item.active {
    background: var(--nav-item-active-bg);
    color: var(--nav-item-active-text);
  }
</style>
