<script lang="ts">
  import { onMount } from 'svelte';
  import { _, locale } from 'svelte-i18n';
  import {
    PageHeader,
    SectionLabel,
    List,
    Row,
    Field,
    Button,
    TextIcon,
  } from '@metocast/design-system';
  import {
    getTitleTemplate,
    setTitleTemplate,
    getSlideFolder,
    setSlideFolder,
    renderTitle,
    pickDirectory,
    hostCapabilities,
    DEFAULT_TITLE_TEMPLATE,
    TITLE_VARIABLES,
  } from '@metocast/core-client';
  import { appMode } from '$lib/core';
  import { pushToast } from '$lib/notifications.svelte';
  import NotifBell from '$lib/NotifBell.svelte';
  import TitlePreview from '$lib/TitlePreview.svelte';

  const TITLE_LIMIT = 100;

  let template = $state(DEFAULT_TITLE_TEMPLATE);
  let templateSaving = $state(false);
  let folder = $state('');
  let folderDraft = $state('');
  let folderSaving = $state(false);

  // Only the desktop shell that *is* the core sees the same filesystem the core
  // writes to, so it gets a native picker; every other window types the path and
  // the core validates it.
  const canPick = $derived(hostCapabilities.dialogs && appMode() === 'server');
  const variables = [...TITLE_VARIABLES.map((v) => `{${v}}`), '{date|YYYY.MM.DD.}'];

  // Sample values, so the preview shows the shape of a real title rather than the raw template.
  const templatePreview = $derived(
    renderTitle(
      template,
      {
        date: new Date(2026, 7, 9, 10, 0),
        title: $_('settings.template.sampleTitle'),
        textus: 'Zsolt 128,1',
        leckio: '128. Zsolt',
        speaker: $_('settings.template.sampleSpeaker'),
      },
      $locale ?? 'en',
    ),
  );

  function toast(title: string, tone: 'ok' | 'error', body?: string) {
    pushToast({ kind: $_('screens.eventSettings.title'), source: 'Core', title, tone, body });
  }

  async function saveTemplate() {
    templateSaving = true;
    try {
      template = (await setTitleTemplate(template.trim() || DEFAULT_TITLE_TEMPLATE)).template;
      toast($_('eventSettings.templateSaved'), 'ok');
    } catch (e) {
      toast($_('eventSettings.saveFailed'), 'error', String(e));
    } finally {
      templateSaving = false;
    }
  }

  async function saveFolder(path: string) {
    folderSaving = true;
    try {
      folder = (await setSlideFolder(path)).path;
      folderDraft = folder;
      toast($_('eventSettings.folderSaved'), 'ok');
    } catch (e) {
      toast($_('eventSettings.saveFailed'), 'error', String(e));
    } finally {
      folderSaving = false;
    }
  }

  async function copyVariable(token: string) {
    try {
      await navigator.clipboard.writeText(token);
      toast($_('eventSettings.copied', { values: { token } }), 'ok');
    } catch (e) {
      toast($_('eventSettings.copyFailed'), 'error', String(e));
    }
  }

  async function pickFolder() {
    const path = await pickDirectory($_('eventSettings.slides.choose'));
    if (path) await saveFolder(path);
  }

  onMount(async () => {
    template = (await getTitleTemplate().catch(() => null))?.template || DEFAULT_TITLE_TEMPLATE;
    folder = (await getSlideFolder().catch(() => null))?.path ?? '';
    folderDraft = folder;
  });
</script>

<PageHeader
  title={$_('screens.eventSettings.title')}
  back={{ label: $_('settings.back'), href: '/settings' }}
>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

<div class="page">
  <SectionLabel hint={$_('eventSettings.slides.hint')}>
    {$_('eventSettings.slides.section')}
  </SectionLabel>
  <List>
    <Row
      title={$_('eventSettings.slides.title')}
      meta={folder || $_('eventSettings.slides.none')}
      chevron={false}
      last={!canPick}
    >
      {#snippet icon()}<TextIcon char="▤" />{/snippet}
      {#snippet control()}
        {#if canPick}
          <Button variant="secondary" compact onclick={pickFolder} disabled={folderSaving}>
            {$_('eventSettings.slides.choose')}
          </Button>
        {:else if folder}
          <Button
            variant="secondary"
            compact
            onclick={() => saveFolder('')}
            disabled={folderSaving}
          >
            {$_('eventSettings.slides.clear')}
          </Button>
        {/if}
      {/snippet}
    </Row>
    {#if !canPick}
      <form
        class="draft"
        onsubmit={(e) => {
          e.preventDefault();
          void saveFolder(folderDraft.trim());
        }}
      >
        <Field
          label={$_('eventSettings.slides.field')}
          bind:value={folderDraft}
          placeholder={$_('eventSettings.slides.placeholder')}
        />
        <Button type="submit" variant="primary" compact disabled={folderSaving}>
          {$_('eventSettings.slides.save')}
        </Button>
      </form>
    {/if}
  </List>

  <SectionLabel hint={$_('settings.template.hint')}>{$_('settings.template.section')}</SectionLabel>
  <List>
    <form
      class="draft"
      onsubmit={(e) => {
        e.preventDefault();
        void saveTemplate();
      }}
    >
      <Field
        label={$_('settings.template.field')}
        bind:value={template}
        placeholder={DEFAULT_TITLE_TEMPLATE}
      />
      <TitlePreview
        label={$_('editor.previewLabel')}
        text={templatePreview}
        count={`${templatePreview.length}/${TITLE_LIMIT}`}
        warn={templatePreview.length > TITLE_LIMIT - 10}
      />
      <p class="template-vars">
        {#each variables as v (v)}
          <button
            type="button"
            onclick={() => copyVariable(v)}
            title={$_('eventSettings.copyHint')}
          >
            {v}
          </button>
        {/each}
        <code>[{$_('settings.template.optional')}]</code>
      </p>
      <div class="template-actions">
        <Button type="submit" variant="primary" compact disabled={templateSaving}>
          {$_('settings.template.save')}
        </Button>
        <Button compact onclick={() => (template = DEFAULT_TITLE_TEMPLATE)}>
          {$_('settings.template.reset')}
        </Button>
      </div>
    </form>
  </List>
</div>

<style>
  .page {
    padding-bottom: 96px;
  }
  .draft {
    padding: 14px 24px 16px;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
  }
  .template-vars {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .template-vars code,
  .template-vars button {
    font-family: var(--type-mono-family, monospace);
    font-size: 10px;
    letter-spacing: 0.02em;
    color: var(--text-muted);
    border: 1px solid color-mix(in srgb, var(--text-primary) 14%, transparent);
    padding: 2px 5px;
  }
  .template-vars button {
    background: none;
    cursor: pointer;
  }
  .template-vars button:hover,
  .template-vars button:focus-visible {
    color: var(--text-primary);
    border-color: color-mix(in srgb, var(--text-primary) 40%, transparent);
  }
  .template-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  @media (min-width: 760px) {
    .page {
      padding-bottom: 32px;
    }
  }
</style>
