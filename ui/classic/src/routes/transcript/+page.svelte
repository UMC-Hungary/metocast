<script lang="ts">
  // The live English transcript for phones in the room. `?overlay=1` is a transparent two-line
  // OBS browser source; `?wsPort=` reaches the core from a dev server. Same page in every UI.
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import {
    connectTranscriptWs,
    type TranscriptSegment,
    type TranscriptionStatus,
  } from '@metocast/core-client';

  const KEEP = 200;
  let segments = $state<TranscriptSegment[]>([]);
  let partial = $state('');
  let status = $state<TranscriptionStatus | null>(null);
  let end = $state<HTMLElement | null>(null);
  const overlay = $page.url.searchParams.get('overlay') === '1';

  const pill = $derived(
    status === 'running' || status === 'connecting'
      ? 'Live'
      : status === 'unavailable'
        ? 'Offline: audio source disconnected'
        : 'Not running',
  );

  onMount(() =>
    connectTranscriptWs({
      wsPort: $page.url.searchParams.get('wsPort'),
      onMessage: (msg) => {
        switch (msg.type) {
          case 'transcription.snapshot': {
            // A reconnect keeps what was received; only newer lines are added.
            const last = segments.at(-1)?.startedAt ?? '';
            segments = [...segments, ...msg.segments.filter((s) => s.startedAt > last)].slice(
              -KEEP,
            );
            partial = msg.partial;
            status = msg.state.status;
            break;
          }
          case 'transcription.state':
            status = msg.state.status;
            break;
          case 'transcription.partial':
            partial = msg.text;
            break;
          case 'transcription.segment':
            segments = [...segments, msg.segment].slice(-KEEP);
            partial = '';
            break;
          case 'transcription.credit':
            break;
        }
      },
    }),
  );

  $effect(() => {
    void segments.length;
    void partial;
    end?.scrollIntoView({ block: 'end' });
  });
</script>

<svelte:head>
  <title>Live transcript</title>
  <meta name="color-scheme" content="light dark" />
</svelte:head>

{#if overlay}
  <main class="overlay">
    {#if segments.length || partial}
      <div class="lines">
        <span
          >{segments.at(-1)?.text ?? ''}
          {#if partial}<span class="partial">{partial}</span>{/if}</span
        >
      </div>
    {/if}
  </main>
{:else}
  <main class="page">
    <header>
      <span class="pill" class:live={pill === 'Live'} class:offline={status === 'unavailable'}
        >{pill}</span
      >
    </header>
    {#if !segments.length && !partial}
      <p class="empty">The English transcript appears here while the sermon is translated.</p>
    {/if}
    {#each segments as segment (segment.startedAt + segment.seq)}
      <p>{segment.text}</p>
    {/each}
    {#if partial}<p class="partial">{partial}</p>{/if}
    <div bind:this={end}></div>
  </main>
{/if}

<style>
  :global(html),
  :global(body) {
    margin: 0;
  }
  /* Follows the system appearance; on body so overscroll matches too. */
  :global(body:has(.page)) {
    --bg: #faf8f3;
    --text: #1c1a17;
    --pill-bg: #e6e1d6;
    --pill-text: #5c574e;
    --live: #b3261e;
    --offline: #8a6400;
    background: var(--bg);
  }
  @media (prefers-color-scheme: dark) {
    :global(body:has(.page)) {
      --bg: #111;
      --text: #f4f1ea;
      --pill-bg: #333;
      --pill-text: #bbb;
      --offline: #6b4e00;
    }
  }
  .page {
    min-height: 100vh;
    box-sizing: border-box;
    padding: 16px 20px 40vh;
    background: var(--bg);
    color: var(--text);
    font-family: system-ui, sans-serif;
    font-size: clamp(22px, 5vw, 34px);
    line-height: 1.4;
  }
  header {
    position: sticky;
    top: 0;
    padding: 8px 0 12px;
    background: var(--bg);
  }
  .pill {
    display: inline-block;
    padding: 4px 12px;
    border-radius: 999px;
    font-size: 14px;
    letter-spacing: 0.5px;
    background: var(--pill-bg);
    color: var(--pill-text);
  }
  .pill.live {
    background: var(--live);
    color: #fff;
  }
  .pill.offline {
    background: var(--offline);
    color: #fff;
  }
  p {
    margin: 0 0 0.6em;
  }
  .partial {
    opacity: 0.55;
  }
  .empty {
    font-size: 18px;
    opacity: 0.6;
  }
  .overlay {
    position: fixed;
    inset: auto 0 0 0;
    padding: 0 5vw 4vh;
    background: transparent;
    color: #fff;
    font-family: system-ui, sans-serif;
    font-size: 5vh;
    line-height: 1.3;
    text-align: center;
    text-shadow:
      0 0 6px #000,
      0 2px 4px #000;
  }
  .lines {
    /* Two lines at most, bottom-aligned, so older words scroll off the top. */
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    max-height: 2.6em;
    overflow: hidden;
  }
  :global(body:has(.overlay)) {
    background: transparent;
  }
</style>
