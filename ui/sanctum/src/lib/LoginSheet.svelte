<script lang="ts">
  import { _ } from 'svelte-i18n';
  import {
    Sheet,
    Field,
    SectionLabel,
    Dot,
    IconButton,
    Button,
    Glyph,
  } from '@metocast/design-system';
  import {
    fetchConnectorConfig,
    saveConnectorConfig,
    openExternal,
    youtubeAuthUrl,
    facebookAuthUrl,
  } from '@metocast/core-client';

  type Provider = 'youtube' | 'facebook';

  let { provider = $bindable<Provider | null>(null) }: { provider: Provider | null } = $props();

  const chars: Record<Provider, string> = { youtube: 'Y', facebook: 'F' };
  const redirects: Record<Provider, string> = {
    youtube: 'metocast://auth/yt',
    facebook: 'metocast://auth/fb',
  };
  const authUrls: Record<Provider, () => Promise<string>> = {
    youtube: youtubeAuthUrl,
    facebook: facebookAuthUrl,
  };

  let view = $state<'login' | 'credentials'>('login');
  let showSecret = $state(false);
  let busy = $state(false);

  let yt = $state({ clientId: '', clientSecret: '', clientSecretSet: false });
  let fb = $state({ appId: '', appSecret: '', appSecretSet: false, pageId: '' });

  const providerName = $derived(
    provider ? provider.charAt(0).toUpperCase() + provider.slice(1) : '',
  );
  const hasCreds = $derived(
    provider === 'youtube'
      ? Boolean(yt.clientId) && yt.clientSecretSet
      : provider === 'facebook'
        ? Boolean(fb.appId) && fb.appSecretSet
        : false,
  );
  const clientLabel = $derived(
    provider === 'youtube' ? yt.clientId : provider === 'facebook' ? fb.appId : '',
  );
  const providerChar = $derived(provider ? chars[provider] : '');
  const providerRedirect = $derived(provider ? redirects[provider] : '');

  const maskMid = (v: string): string => (v.length <= 6 ? v : `${v.slice(0, 3)}…${v.slice(-3)}`);

  async function load(p: Provider): Promise<void> {
    if (p === 'youtube') {
      const c = await fetchConnectorConfig('youtube').catch(() => null);
      if (c)
        yt = {
          clientId: c.clientId,
          clientSecret: c.clientSecret,
          clientSecretSet: Boolean(c.clientSecretSet),
        };
    } else {
      const c = await fetchConnectorConfig('facebook').catch(() => null);
      if (c)
        fb = {
          appId: c.appId,
          appSecret: c.appSecret,
          appSecretSet: Boolean(c.appSecretSet),
          pageId: c.pageId,
        };
    }
  }

  $effect(() => {
    const p = provider;
    if (!p) return;
    view = 'login';
    showSecret = false;
    load(p);
  });

  const close = (): void => {
    provider = null;
  };

  async function continueOauth(): Promise<void> {
    if (!provider || !hasCreds) return;
    busy = true;
    await openExternal(await authUrls[provider]()).catch(() => {});
    busy = false;
    close();
  }

  async function saveYoutube(): Promise<void> {
    const c = await fetchConnectorConfig('youtube').catch(() => null);
    if (!c) return;
    await saveConnectorConfig('youtube', {
      ...c,
      clientId: yt.clientId,
      clientSecret: yt.clientSecret,
    });
    await load('youtube');
    view = 'login';
  }

  async function saveFacebook(): Promise<void> {
    const c = await fetchConnectorConfig('facebook').catch(() => null);
    if (!c) return;
    await saveConnectorConfig('facebook', {
      ...c,
      appId: fb.appId,
      appSecret: fb.appSecret,
      pageId: fb.pageId,
    });
    await load('facebook');
    view = 'login';
  }
</script>

{#if provider}
  <Sheet
    open
    title={providerName}
    eyebrow={view === 'login' ? $_('login.signIn') : $_('login.credentials')}
    ariaLabel={`${providerName} ${view === 'login' ? $_('login.signIn') : $_('login.credentials')}`}
    onclose={close}
  >
    {#snippet leading()}<Glyph char={providerChar} size={34} />{/snippet}
    {#snippet action()}
      <IconButton
        icon="gear"
        label={$_('login.credentials')}
        variant="circle"
        onclick={() => (view = view === 'login' ? 'credentials' : 'login')}
      />
    {/snippet}

    {#if view === 'login'}
      <p class="copy">{$_(`login.scope.${provider}`)}</p>
      <div class="cred">
        <Dot color={hasCreds ? 'var(--status-ok)' : 'var(--status-warn)'} size={6} />
        <span
          >{hasCreds
            ? `${$_('login.clientId')} · ${maskMid(clientLabel)}`
            : $_('login.noCreds')}</span
        >
        <button class="link" onclick={() => (view = 'credentials')}>
          {hasCreds ? $_('login.edit') : $_('login.add')}
        </button>
      </div>
      <SectionLabel>{$_('login.account')}</SectionLabel>
      <div class="fields">
        <Field label={$_('login.email')} placeholder={`you@${provider}.com`} />
        <Field
          label={$_('login.password')}
          type="password"
          placeholder="••••••••••"
          hint={$_('login.passwordHint')}
        />
      </div>
      <div class="commit">
        <Button variant="primary" block disabled={busy || !hasCreds} onclick={continueOauth}>
          {busy
            ? $_('login.connecting')
            : `${$_('login.continue', { values: { provider: providerName } })} →`}
        </Button>
      </div>
    {:else}
      <p class="copy">{$_('login.credsIntro', { values: { provider: providerName } })}</p>
      <SectionLabel hint="OAuth 2.0">{$_('login.credentials')}</SectionLabel>
      <div class="fields">
        {#if provider === 'youtube'}
          <Field
            label={$_('login.field.clientId')}
            bind:value={yt.clientId}
            placeholder="youtube-XXXXXXXXXXXX"
          />
          <Field
            label={$_('login.field.clientSecret')}
            bind:value={yt.clientSecret}
            type={showSecret ? 'text' : 'password'}
            placeholder="••••••••••••"
          >
            {#snippet trailing()}
              <button class="link" onclick={() => (showSecret = !showSecret)}>
                {showSecret ? $_('login.hide') : $_('login.show')}
              </button>
            {/snippet}
          </Field>
        {:else}
          <Field
            label={$_('login.field.appId')}
            bind:value={fb.appId}
            placeholder="facebook-XXXXXXXXXXXX"
          />
          <Field
            label={$_('login.field.appSecret')}
            bind:value={fb.appSecret}
            type={showSecret ? 'text' : 'password'}
            placeholder="••••••••••••"
          >
            {#snippet trailing()}
              <button class="link" onclick={() => (showSecret = !showSecret)}>
                {showSecret ? $_('login.hide') : $_('login.show')}
              </button>
            {/snippet}
          </Field>
          <Field
            label={$_('login.field.pageId')}
            bind:value={fb.pageId}
            placeholder="1000000000000"
          />
        {/if}
        <Field
          label={$_('login.redirectUri')}
          value={providerRedirect}
          readonly
          hint={$_('login.redirectHint')}
        />
      </div>
      <div class="actions">
        <Button variant="secondary" block onclick={() => (view = 'login')}
          >{$_('login.cancel')}</Button
        >
        <Button
          variant="primary"
          block
          onclick={provider === 'youtube' ? saveYoutube : saveFacebook}
        >
          {$_('login.saveCreds')}
        </Button>
      </div>
    {/if}
  </Sheet>
{/if}

<style>
  .copy {
    padding: 22px 24px 8px;
    margin: 0;
    font-family: var(--font-display);
    font-style: italic;
    font-size: 15px;
    color: var(--text-secondary, var(--text-muted));
    line-height: 1.45;
  }
  .cred {
    margin: 8px 24px 0;
    border: 1px solid color-mix(in srgb, var(--text-primary) 10%, transparent);
    padding: 12px 14px;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .cred span {
    flex: 1;
    color: var(--text-primary);
    font-size: 13px;
  }
  .link {
    background: transparent;
    border: 0;
    cursor: pointer;
    color: var(--text-primary);
    font-family: var(--font-label);
    font-size: 10px;
    letter-spacing: 1.4px;
    text-transform: uppercase;
  }
  .fields {
    padding: 0 24px;
  }
  .commit {
    padding: 24px;
  }
  .actions {
    display: flex;
    gap: 10px;
    padding: 20px 24px 24px;
  }
</style>
