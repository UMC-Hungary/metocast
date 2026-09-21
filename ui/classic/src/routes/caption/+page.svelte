<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';

  const type = $derived($page.url.searchParams.get('type') ?? 'preview');
  const name = $derived($page.url.searchParams.get('bold') ?? '');
  const showLogo = $derived($page.url.searchParams.get('showLogo') === 'true');
  const logoAlt = $derived($page.url.searchParams.get('alt') ?? '');

  onMount(() => {
    document.documentElement.style.background = 'rgb(255, 255, 255)';
    document.body.style.background = 'rgb(255, 255, 255)';
    document.body.style.overflow = 'visible';
    document.documentElement.style.overflow = 'visible';
    return () => {
      document.documentElement.style.background = '';
      document.body.style.background = '';
      document.body.style.overflow = '';
      document.documentElement.style.overflow = '';
    };
  });
</script>

<svelte:head>
  <link
    href="https://fonts.googleapis.com/css2?family=Oswald:wght@400;700&display=swap"
    rel="stylesheet"
  />
</svelte:head>

{#if type === 'preview'}
  <main class="min-h-screen flex items-center justify-center">
    <div class="aspect-container">
      <div class="content">
        <h1 class="name-title">{name}</h1>
        <div class="service-info">
          <span>VASÁRNAPI ISTENTISZTELET</span>
          <span class="dot"></span>
          <span>IGEHIRDETÉS</span>
        </div>
      </div>

      {#if showLogo}
        <div class="logo-container">
          <img
            src="/logo.svg"
            alt={logoAlt}
            width="400"
            height="120"
            decoding="async"
            data-nimg="1"
            style="color: transparent;"
          />
        </div>
      {/if}
    </div>
  </main>
{/if}

<style>
  /* ── Page reset rules ── */
  :global(html) {
    line-height: 1.5;
    -webkit-text-size-adjust: 100%;
  }

  :global(h1, h2, h3, h4, h5, h6) {
    font-size: inherit;
    font-weight: inherit;
  }

  /* ── Custom base (app/globals.css) ── */
  :global(:root) {
    --foreground-rgb: 0, 0, 0;
    --background-rgb: 255, 255, 255;
    --accent-rgb: 220, 38, 38;
  }

  :global(body) {
    font-family: 'Oswald', sans-serif;
    color: rgb(var(--foreground-rgb));
    background: rgb(var(--background-rgb));
    line-height: inherit;
    margin: 0;
    padding: 0;
  }

  /* ── Utility classes ── */
  .min-h-screen {
    min-height: 100vh;
  }

  .flex {
    display: flex;
  }

  .items-center {
    align-items: center;
  }

  .justify-center {
    justify-content: center;
  }

  /* ── Component classes (app/globals.css) ── */
  .aspect-container {
    /* 16:9 aspect ratio container */
    aspect-ratio: 16 / 9;
    width: 100%;
    height: 100vh;
    max-height: 100vh;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 5% 10%;
    box-sizing: border-box;
  }

  .name-title {
    font-size: clamp(2rem, 10vw, 8rem);
    font-weight: 700;
    line-height: 1;
    margin: 0;
    padding: 0;
  }

  .service-info {
    color: rgb(var(--accent-rgb));
    font-size: clamp(1rem, 3vw, 2rem);
    font-weight: 700;
    margin-top: clamp(1rem, 2vw, 2rem);
    display: flex;
    align-items: center;
    gap: 0.5em;
  }

  .dot {
    display: inline-block;
    width: 0.5em;
    height: 0.5em;
    border-radius: 50%;
    background-color: rgb(var(--accent-rgb));
  }

  .logo-container {
    width: 100%;
    max-width: 400px;
    margin-top: auto;
  }

  .logo-container img {
    display: block;
    max-width: 100%;
    width: 100%;
    height: auto;
  }
</style>
