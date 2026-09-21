<script lang="ts">
  import { onMount } from 'svelte';
  import { _, locale } from 'svelte-i18n';
  import {
    PageHeader,
    SectionLabel,
    List,
    Row,
    OverviewCell,
    TextIcon,
    Glyph,
    Toggle,
    Field,
    Checkbox,
    Segmented,
    Button,
    FlameMark,
    Sheet,
    RadioGroup,
  } from '@metocast/design-system';
  import QRCode from 'qrcode';
  import {
    getAppMode,
    getAppVersion,
    resetSetup,
    checkForUpdates,
    sendWsCommand,
    listCronJobs,
    createCronJob,
    updateCronJob,
    fetchConnectorStatuses,
    getLocalHost,
    getServerPort,
    getToken,
    connectLink,
  } from '@metocast/core-client';
  import type { CronJob, UpdateInfo } from '@metocast/core-client';
  import { scheme, setScheme, type Scheme } from '$lib/scheme.svelte';
  import { setLocale, locales } from '$lib/i18n';
  import { hasToken } from '$lib/core';
  import LoginSheet from '$lib/LoginSheet.svelte';
  import UiSwitcher from '$lib/UiSwitcher.svelte';
  import { live } from '$lib/live.svelte';
  import NotifBell from '$lib/NotifBell.svelte';

  let mode = $state<'server' | 'client'>('server');
  let connectQr = $state<string | null>(null);
  let cronJobs = $state<CronJob[]>([]);
  let signedIn = $state<Record<string, boolean>>({ youtube: false, facebook: false });
  const webPresenter = $derived(live.useWebPresenter);
  let update = $state<UpdateInfo | null>(null);
  let appVersion = $state('');
  let draft = $state({ name: '', cronExpression: '', pullYoutube: false, autoUpload: false });

  // The backend cron job must do at least one thing; a job with neither is meaningless.
  const canSaveCron = $derived(
    draft.name.trim().length > 0 &&
      draft.cronExpression.trim().length > 0 &&
      (draft.pullYoutube || draft.autoUpload),
  );

  const accounts = [
    { id: 'youtube' as const, name: 'YouTube', char: 'Y' },
    { id: 'facebook' as const, name: 'Facebook', char: 'F' },
  ];

  let loginProvider = $state<'youtube' | 'facebook' | null>(null);
  let appearanceOpen = $state(false);
  let modeSheetOpen = $state(false);
  let switchArmed = $state(false);

  const signedCount = $derived(Object.values(signedIn).filter(Boolean).length);
  const enabledJobs = $derived(cronJobs.filter((j) => j.enabled).length);
  const schemeOptions = $derived([
    { value: 'light' as Scheme, label: $_('settings.scheme.light') },
    { value: 'dark' as Scheme, label: $_('settings.scheme.dark') },
    { value: 'auto' as Scheme, label: $_('settings.scheme.auto') },
  ]);
  const currentSchemeLabel = $derived(schemeOptions.find((o) => o.value === scheme())?.label ?? '');

  function cronTags(job: CronJob): string {
    const tags = [];
    if (job.pullYoutube) tags.push($_('settings.cron.tagYoutube'));
    if (job.autoUpload) tags.push($_('settings.cron.tagUpload'));
    if (tags.length === 0) tags.push($_('settings.cron.tagCustom'));
    return tags.join(' · ');
  }

  async function refreshCron() {
    try {
      cronJobs = await listCronJobs();
    } catch {
      /* offline — keep what we have */
    }
  }

  onMount(async () => {
    try {
      mode = (await getAppMode()) ?? 'server';
      appVersion = (await getAppVersion()) ?? '';
    } catch {
      /* no host */
    }
    if (mode === 'server') {
      try {
        const [host, port, token] = await Promise.all([
          getLocalHost(),
          getServerPort(),
          getToken(),
        ]);
        if (host && port && token) {
          const payload = connectLink({ url: `http://${host}:${port}`, token });
          connectQr = await QRCode.toDataURL(payload, {
            width: 160,
            margin: 1,
            color: { dark: '#000', light: '#fff' },
          });
        }
      } catch {
        /* server unavailable */
      }
    }
    await refreshCron();
    try {
      const s = await fetchConnectorStatuses();
      signedIn = {
        youtube: s.youtube?.type === 'connected',
        facebook: s.facebook?.type === 'connected',
      };
    } catch {
      /* offline */
    }
  });

  function toggleWebPresenter(enabled: boolean) {
    sendWsCommand('presentation.set_use_web_presenter', { enabled });
  }

  async function toggleCron(job: CronJob, enabled: boolean) {
    await updateCronJob(job.id, {
      name: job.name,
      cronExpression: job.cronExpression,
      enabled,
      pullYoutube: job.pullYoutube,
      autoUpload: job.autoUpload,
    });
    await refreshCron();
  }

  async function addCron() {
    if (!canSaveCron) return;
    await createCronJob({
      name: draft.name,
      cronExpression: draft.cronExpression,
      enabled: true,
      pullYoutube: draft.pullYoutube,
      autoUpload: draft.autoUpload,
    });
    draft = { name: '', cronExpression: '', pullYoutube: false, autoUpload: false };
    await refreshCron();
  }

  async function checkUpdate() {
    try {
      update = await checkForUpdates();
    } catch {
      /* host unavailable */
    }
  }

  function openModeSheet() {
    switchArmed = false;
    modeSheetOpen = true;
  }

  // Desktop restarts inside resetSetup and never returns here.
  async function doResetSetup() {
    try {
      await resetSetup();
    } catch {
      /* host unavailable */
      modeSheetOpen = false;
      return;
    }
    location.reload();
  }
</script>

<PageHeader eyebrow={$_('settings.eyebrow')} title={$_('screens.settings.title')}>
  {#snippet trailing()}<NotifBell />{/snippet}
</PageHeader>

<div class="settings-workspace">
  <div class="primary-col">
    <section class="overview">
      <OverviewCell
        label={$_('settings.overview.mode')}
        value={mode === 'server' ? 'S' : 'C'}
        color="var(--status-ok)"
      />
      <OverviewCell
        label={$_('settings.overview.accounts')}
        value={`${signedCount}/2`}
        color={signedCount === 2 ? 'var(--status-ok)' : 'var(--status-warn)'}
        divider
      />
      <OverviewCell
        label={$_('settings.overview.jobs')}
        value={enabledJobs}
        color="var(--text-muted)"
        divider
      />
    </section>

    <UiSwitcher />

    <SectionLabel>{$_('settings.language')}</SectionLabel>
    <List>
      <Row
        title={$_('settings.languageRow.title')}
        meta={$_('settings.languageRow.meta')}
        chevron={false}
        last
      >
        {#snippet icon()}<TextIcon char="A" />{/snippet}
        {#snippet control()}
          <Segmented
            compact
            label={$_('settings.language')}
            value={$locale ?? 'en'}
            options={locales.map((l) => ({ value: l.code, label: l.code.toUpperCase() }))}
            onchange={(code) => setLocale(code)}
          />
        {/snippet}
      </Row>
    </List>

    {#if hasToken()}
      <SectionLabel hint={mode}>{$_('settings.modeSection')}</SectionLabel>
      <List>
        <Row
          title={$_('settings.mode.title', { values: { mode } })}
          meta={mode === 'client' ? $_('settings.mode.clientMeta') : $_('settings.mode.meta')}
          chevron={false}
          last
        >
          {#snippet icon()}<TextIcon char={mode === 'server' ? 'S' : 'C'} />{/snippet}
          {#snippet control()}
            <Button
              variant={mode === 'client' ? 'danger' : 'secondary'}
              compact
              onclick={openModeSheet}
            >
              {mode === 'client' ? $_('settings.mode.removeAccess') : $_('settings.mode.change')}
            </Button>
          {/snippet}
        </Row>
      </List>
    {/if}

    {#if mode === 'server' && hasToken()}
      <SectionLabel hint={$_('settings.connect.hint')}
        >{$_('settings.connect.section')}</SectionLabel
      >
      <List>
        <Row
          title={$_('settings.connect.section')}
          meta={$_('settings.connect.hint')}
          chevron={false}
          last
        >
          {#snippet icon()}<TextIcon char="⠿" />{/snippet}
          {#snippet control()}
            {#if connectQr}
              <img
                src={connectQr}
                alt="QR code to connect a device"
                width="80"
                height="80"
                style="display:block"
              />
            {:else}
              <span class="qr-loading">{$_('settings.connect.loading')}</span>
            {/if}
          {/snippet}
        </Row>
      </List>
    {/if}

    {#if hasToken()}
      <SectionLabel hint={$_('settings.account.signedCount', { values: { n: signedCount } })}>
        {$_('settings.accountsSection')}
      </SectionLabel>
      <List>
        {#each accounts as acc, i (acc.id)}
          <Row
            title={acc.name}
            meta={signedIn[acc.id] ? $_('settings.account.ready') : $_('settings.account.needsKey')}
            chevron={false}
            last={i === accounts.length - 1}
          >
            {#snippet icon()}<Glyph char={acc.char} size={34} />{/snippet}
            {#snippet control()}
              <Button variant="secondary" compact onclick={() => (loginProvider = acc.id)}>
                {signedIn[acc.id] ? $_('settings.account.manage') : $_('settings.account.login')}
              </Button>
            {/snippet}
          </Row>
        {/each}
      </List>
    {/if}
  </div>

  <aside class="side-col">
    <div class="side-head">
      <SectionLabel hint="7">{$_('settings.connectors')}</SectionLabel>
    </div>
    <List>
      <Row
        title={$_('settings.connectorsRow.title')}
        meta={$_('settings.connectorsRow.meta')}
        detail={$_('settings.connectorsRow.open')}
        href="/settings/connectors"
        last
      >
        {#snippet icon()}<TextIcon char="↳" />{/snippet}
      </Row>
    </List>

    <div class="side-head">
      <SectionLabel>{$_('screens.eventSettings.title')}</SectionLabel>
    </div>
    <div class="side-stack">
      <List>
        <Row
          title={$_('settings.eventsRow.title')}
          meta={$_('settings.eventsRow.meta')}
          detail={$_('settings.connectorsRow.open')}
          href="/settings/events"
          last
        >
          {#snippet icon()}<TextIcon char="↳" />{/snippet}
        </Row>
      </List>
    </div>

    <SectionLabel>{$_('settings.presentations')}</SectionLabel>
    <List>
      <Row
        title={$_('settings.webPresenter.title')}
        meta={$_('settings.webPresenter.meta')}
        chevron={false}
        last
      >
        {#snippet icon()}<TextIcon char="▣" />{/snippet}
        {#snippet control()}
          <Toggle
            checked={webPresenter}
            label={$_('settings.webPresenter.title')}
            onchange={toggleWebPresenter}
          />
        {/snippet}
      </Row>
    </List>

    {#if hasToken()}
      <SectionLabel hint={$_('settings.cron.configured', { values: { n: cronJobs.length } })}>
        {$_('settings.cron.section')}
      </SectionLabel>
      <List>
        {#each cronJobs as job (job.id)}
          <Row title={job.name} meta={`${job.cronExpression} · ${cronTags(job)}`} chevron={false}>
            {#snippet icon()}<TextIcon char={job.enabled ? '✓' : '·'} />{/snippet}
            {#snippet control()}
              <Toggle checked={job.enabled} label={job.name} onchange={(v) => toggleCron(job, v)} />
            {/snippet}
          </Row>
        {/each}
        <form
          class="draft"
          onsubmit={(e) => {
            e.preventDefault();
            addCron();
          }}
        >
          <Field
            label={$_('settings.cron.name')}
            bind:value={draft.name}
            placeholder={$_('settings.cron.namePlaceholder')}
          />
          <Field
            label={$_('settings.cron.expr')}
            bind:value={draft.cronExpression}
            placeholder={$_('settings.cron.exprPlaceholder')}
          />
          <div class="cron-options">
            <Checkbox label={$_('settings.cron.pullYoutube')} bind:checked={draft.pullYoutube} />
            <Checkbox label={$_('settings.cron.autoUpload')} bind:checked={draft.autoUpload} />
          </div>
          <Button type="submit" variant="primary" block disabled={!canSaveCron}>
            {$_('settings.cron.save')}
          </Button>
        </form>
      </List>
    {/if}

    <SectionLabel>{$_('settings.app')}</SectionLabel>
    <List>
      <Row
        title={$_('settings.appearance')}
        detail={currentSchemeLabel}
        onclick={() => (appearanceOpen = true)}
        last={!hasToken()}
      >
        {#snippet icon()}<TextIcon char={scheme() === 'dark' ? '☾' : '☀'} />{/snippet}
      </Row>
      <!-- The log, the version and the update all read the core's own machine, so
           they need a token to reach it. -->
      {#if hasToken()}
        <Row
          title={$_('logs.title')}
          meta={$_('logs.rowMeta')}
          detail={$_('logs.open')}
          href="/settings/logs"
        >
          {#snippet icon()}<TextIcon char="≡" />{/snippet}
        </Row>
        <Row
          title={$_('settings.version.title')}
          meta={update?.latestVersion
            ? $_('settings.version.available', { values: { v: update.latestVersion } })
            : appVersion}
          detail={update?.latestVersion
            ? $_('settings.version.download')
            : $_('settings.version.check')}
          chevron={false}
          onclick={checkUpdate}
          last
        >
          {#snippet icon()}<TextIcon char="↓" />{/snippet}
        </Row>
      {/if}
    </List>

    <footer>
      <FlameMark size={18} />
      <span>Metocast</span>
    </footer>
  </aside>
</div>

<LoginSheet bind:provider={loginProvider} />

<Sheet
  bind:open={modeSheetOpen}
  title={mode === 'client' ? $_('settings.mode.removeAccess') : $_('settings.mode.switchTitle')}
  ariaLabel={mode === 'client' ? $_('settings.mode.removeAccess') : $_('settings.mode.switchTitle')}
  onclose={() => (switchArmed = false)}
>
  <div class="mode-switch">
    {#if mode === 'client'}
      <p>{$_('settings.mode.clientMeta')}</p>
      <Button variant="danger" block onclick={doResetSetup}>
        {$_('settings.mode.removeAccess')}
      </Button>
    {:else if !switchArmed}
      <p>{$_('settings.mode.switchBody')}</p>
      <Button variant="danger" block onclick={() => (switchArmed = true)}>
        {$_('settings.mode.switchConfirm')}
      </Button>
    {:else}
      <p>{$_('settings.mode.switchConfirmDialog')}</p>
      <Button variant="danger" block onclick={doResetSetup}>
        {$_('settings.mode.switchConfirmFinal')}
      </Button>
    {/if}
  </div>
</Sheet>

<Sheet
  bind:open={appearanceOpen}
  title={$_('settings.appearance')}
  ariaLabel={$_('settings.appearance')}
>
  <div class="appearance-options">
    <RadioGroup
      label={$_('settings.appearance')}
      labelHidden
      value={scheme()}
      options={schemeOptions}
      onchange={(v) => {
        setScheme(v);
        appearanceOpen = false;
      }}
    />
  </div>
</Sheet>

<style>
  .overview {
    margin: 0 24px;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    border-block: 1px solid color-mix(in srgb, var(--text-primary) 12%, transparent);
  }
  .settings-workspace,
  .primary-col,
  .side-col {
    display: contents;
  }
  .draft {
    padding: 14px 24px 16px;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
  }
  .cron-options {
    display: grid;
    gap: 6px;
  }
  .appearance-options {
    padding: 20px 24px 24px;
  }
  .mode-switch {
    padding: 20px 24px 24px;
    display: grid;
    gap: 16px;
  }
  .mode-switch p {
    margin: 0;
    font-size: 14px;
    color: var(--text-muted);
    line-height: 1.45;
  }
  .qr-loading {
    font-family: var(--font-label);
    font-size: 10px;
    letter-spacing: 1px;
    color: var(--text-muted);
  }
  footer {
    text-align: center;
    padding: 36px 0 128px;
    color: var(--text-primary);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  footer span {
    font-family: var(--font-display);
    font-style: italic;
    font-size: 17px;
    font-weight: 500;
  }

  @media (min-width: 760px) {
    .settings-workspace {
      display: grid;
      grid-template-columns: minmax(320px, 1fr) minmax(280px, 340px);
      gap: 0 18px;
      align-items: start;
      padding: 0 18px 64px;
    }
    .primary-col,
    .side-col {
      display: block;
      min-width: 0;
    }
    .side-col {
      position: sticky;
      top: 18px;
    }
    .side-head {
      display: none;
    }
    .side-stack {
      margin-top: calc(var(--ui-border-hairline) * -1);
    }
    .overview {
      margin: 0;
    }
    footer {
      padding-bottom: 32px;
    }
  }
  @media (min-width: 1360px) {
    .settings-workspace {
      grid-template-columns: minmax(560px, 1fr) 380px;
      gap: 0 30px;
      padding-inline: 32px;
    }
  }
</style>
