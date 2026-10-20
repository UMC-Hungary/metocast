<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { List, Row, Select, EmptyState } from '@metocast/design-system';
  import { searchFiles, listFolders } from '@metocast/core-client';
  import type {
    HighlightPart,
    PptFile,
    PptFolder,
  } from '@metocast/core-client/schemas/ws-messages';
  import { live } from '$lib/live.svelte';

  /**
   * Every deck in the watched folders, narrowed by a search and a folder. A click hands the deck
   * to `onpick`: the Presentations page opens it, the Order of Service adds it as a song.
   */
  interface Props {
    onpick: (file: PptFile) => void;
    /** The deck on screen, marked in the list. */
    openPath?: string | null;
  }

  let { onpick, openPath = null }: Props = $props();

  let query = $state('');
  let folderId = $state('all');
  let files = $state<PptFile[]>([]);
  let folders = $state<PptFolder[]>([]);

  $effect(() => {
    const filter = query;
    const folder = folderId === 'all' ? undefined : folderId;
    // Search again when the core finishes indexing, so new decks show up.
    void live.slideIndexRevision;
    // Typing shouldn't ask the server on every keystroke.
    const timer = setTimeout(
      async () => {
        files = await searchFiles(
          filter,
          folder ? { folderId: folder, limit: 100 } : { limit: 100 },
        );
      },
      filter ? 250 : 0,
    );
    return () => clearTimeout(timer);
  });

  $effect(() => {
    listFolders().then((list) => (folders = list));
  });

  const options = $derived([
    { value: 'all', label: $_('library.allFolders') },
    ...folders.map((folder) => ({ value: folder.id, label: folder.name })),
  ]);

  function folderName(id: string): string {
    return folders.find((folder) => folder.id === id)?.name ?? '';
  }
</script>

{#snippet marked(parts: HighlightPart[])}
  {#each parts as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
{/snippet}

<div class="library">
  <div class="filters">
    <input
      class="search"
      type="search"
      bind:value={query}
      placeholder={$_('library.searchPlaceholder')}
      aria-label={$_('library.search')}
    />
    <Select {options} bind:value={folderId} label={$_('library.folder')} />
  </div>
  {#if files.length > 0}
    <List as="ul" aria-label={$_('library.title')}>
      {#each files as file, index (file.id)}
        <Row
          meta={folderName(file.folderId)}
          detail={file.number === null ? '' : String(file.number)}
          current={file.path === openPath ? 'true' : false}
          last={index === files.length - 1}
          onclick={() => onpick(file)}
        >
          {@render marked(file.titleParts)}
          {#if file.snippetParts.length > 0}
            <span class="snippet">{@render marked(file.snippetParts)}</span>
          {/if}
        </Row>
      {/each}
    </List>
  {:else if folders.length === 0}
    <EmptyState title={$_('library.noFolders')} hint={$_('library.noFoldersHint')} />
  {:else}
    <EmptyState title={$_('library.empty')} hint={$_('library.emptyHint')} />
  {/if}
</div>

<style>
  .library {
    display: grid;
    gap: 12px;
  }
  .filters {
    display: grid;
    grid-template-columns: 1fr minmax(140px, 220px);
    gap: 8px;
    align-items: center;
  }
  .search {
    min-height: var(--ui-target-min);
    padding: 0 12px;
    border: var(--border-1) solid var(--border-control);
    border-radius: var(--radius-8);
    background: var(--surface-raised);
    color: var(--text-primary);
    font: inherit;
  }
  mark {
    background: none;
    color: inherit;
    font-weight: 600;
    text-decoration: underline;
    text-decoration-color: var(--accent);
    text-underline-offset: 3px;
  }
  .snippet {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    margin-top: 2px;
    font-size: var(--type-caption-size);
    color: var(--text-muted);
  }
  @media (max-width: 520px) {
    .filters {
      grid-template-columns: 1fr;
    }
  }
</style>
