<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { _ } from 'svelte-i18n';
  import { Icon, Lockup } from '@metocast/design-system';
  import { installNativeNav, setNativeNavActive } from '@metocast/core-client';

  // One nav over the primary destinations. Event editor lives under events,
  // connectors under settings, so those keep their parent tab active.
  const tabs = [
    {
      href: '/',
      icon: 'home',
      symbol: 'house',
      key: 'dashboard',
      active: (p: string) => p === '/',
    },
    {
      href: '/events',
      icon: 'calendar',
      symbol: 'calendar',
      key: 'events',
      active: (p: string) => p.startsWith('/events'),
    },
    {
      href: '/presentations',
      icon: 'slides',
      symbol: 'rectangle.on.rectangle',
      key: 'slides',
      active: (p: string) => p.startsWith('/presentations'),
    },
    {
      href: '/settings',
      icon: 'gear',
      symbol: 'gearshape',
      key: 'settings',
      active: (p: string) => p.startsWith('/settings'),
    },
  ] as const;

  const path = $derived($page.url.pathname);
  const activeIndex = $derived(
    Math.max(
      0,
      tabs.findIndex((t) => t.active(path)),
    ),
  );

  // On Apple platforms the OS draws the nav itself, in real liquid glass.
  let native = $state(false);

  onMount(async () => {
    const placement = await installNativeNav(
      tabs.map((t) => ({ label: $_(`nav.${t.key}`), symbol: t.symbol })),
      activeIndex,
      (index) => {
        const tab = tabs[index];
        if (tab) goto(tab.href);
      },
    );
    native = placement !== null;
    if (placement) document.body.classList.add(`native-nav-${placement}`);
  });

  $effect(() => {
    if (native) setNativeNavActive(activeIndex);
  });
</script>

{#if !native}
  <nav>
    <div class="brand"><Lockup name="Metocast" /></div>
    {#each tabs as tab (tab.href)}
      <a
        href={tab.href}
        class:active={tab.active(path)}
        aria-current={tab.active(path) ? 'page' : undefined}
      >
        <Icon name={tab.icon} size={22} />
        <span>{$_(`nav.${tab.key}`)}</span>
      </a>
    {/each}
  </nav>
{/if}

<style>
  nav {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: var(--z-nav);
    display: flex;
    justify-content: space-around;
    align-items: center;
    background: var(--surface-base);
    border-top: 1px solid color-mix(in srgb, var(--text-primary) 12%, transparent);
    padding: 10px 6px max(14px, env(safe-area-inset-bottom, 14px));
  }

  .brand {
    display: none;
  }

  a {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px;
    text-decoration: none;
    color: var(--text-muted);
  }

  a.active {
    color: var(--text-primary);
  }

  span {
    font-family: var(--font-label);
    font-size: 9px;
    letter-spacing: 1.2px;
    text-transform: uppercase;
  }

  a.active span {
    font-weight: 500;
  }

  /* The bar floats as a translucent capsule and content passes under it. */
  @media (max-width: 979px) {
    nav {
      left: 12px;
      right: 12px;
      bottom: max(10px, env(safe-area-inset-bottom, 10px));
      padding: 5px;
      border: 1px solid color-mix(in srgb, var(--surface-inverse) 8%, transparent);
      border-radius: var(--ui-radius-pill);
      background: color-mix(in srgb, var(--surface-base) 58%, transparent);
      -webkit-backdrop-filter: blur(24px) saturate(180%);
      backdrop-filter: blur(24px) saturate(180%);
      box-shadow:
        inset 0 1px 0 color-mix(in srgb, var(--surface-raised) 70%, transparent),
        0 8px 28px var(--shadow-overlay);
    }

    nav a {
      padding: 7px 4px;
      border-radius: var(--ui-radius-pill);
    }

    nav a.active {
      background: color-mix(in srgb, var(--surface-raised) 65%, transparent);
    }
  }

  /* Side navigation above the mobile breakpoint. */
  @media (min-width: 980px) {
    nav {
      top: 0;
      right: auto;
      bottom: 0;
      width: 226px;
      flex-direction: column;
      align-items: stretch;
      justify-content: flex-start;
      gap: 4px;
      padding: 22px 14px;
      border-top: 0;
      border-right: 1px solid color-mix(in srgb, var(--text-primary) 12%, transparent);
    }

    .brand {
      display: block;
      padding: 2px 8px 22px;
      margin-bottom: 12px;
      border-bottom: 1px solid color-mix(in srgb, var(--text-primary) 12%, transparent);
    }

    a {
      flex: 0 0 auto;
      min-height: 44px;
      flex-direction: row;
      justify-content: flex-start;
      gap: 12px;
      padding: 10px 9px;
      border-left: 2px solid transparent;
      /* Square, per the reference — a flat background with a left accent, no rounding. */
    }

    a.active {
      background: var(--surface-raised);
      border-left-color: var(--text-primary);
    }

    span {
      font-size: 10px;
      letter-spacing: 1.4px;
    }
  }
</style>
