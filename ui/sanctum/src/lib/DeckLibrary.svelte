<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { List, Row, Select, EmptyState } from '@metocast/design-system';
  import { searchFiles, listFolders } from '@metocast/core-client';
  import type { PptFile, PptFolder } from '@metocast/core-client/schemas/ws-messages';

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
    // Typing shouldn't ask the server on every keystroke.
    const timer = setTimeout(
      async () => {
        files = await searchFiles(filter);
      },
      filter ? 250 : 0,
    );
    return () => clearTimeout(timer);
  });

  $effect(() => {
    listFolders().then((list) => (folders = list));
  });

  const shown = $derived(
    folderId === 'all' ? files : files.filter((file) => file.folderId === folderId),
  );
  const options = $derived([
    { value: 'all', label: $_('library.allFolders') },
    ...folders.map((folder) => ({ value: folder.id, label: folder.name })),
  ]);

  function folderName(id: string): string {
    return folders.find((folder) => folder.id === id)?.name ?? '';
  }
</script>

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
  {#if shown.length > 0}
    <List as="ul" aria-label={$_('library.title')}>
      {#each shown as file, index (file.id)}
        <Row
          title={file.name.replace(/\.pptx?$/i, '')}
          meta={folderName(file.folderId)}
          current={file.path === openPath ? 'true' : false}
          last={index === shown.length - 1}
          onclick={() => onpick(file)}
        />
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
  @media (max-width: 520px) {
    .filters {
      grid-template-columns: 1fr;
    }
  }
</style>
