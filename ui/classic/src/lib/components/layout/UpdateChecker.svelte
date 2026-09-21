<script lang="ts">
  import { onMount } from 'svelte';
  import * as host from '@metocast/core-client';
  import { toast } from 'svelte-sonner';
  import { _ } from 'svelte-i18n';
  import { updaterStore } from '$lib/stores/updater.js';
  import { get } from 'svelte/store';

  async function installUpdate() {
    const t = get(_);
    updaterStore.update((s) => ({ ...s, status: 'installing', error: null }));
    try {
      await host.installUpdate();
      updaterStore.update((s) => ({ ...s, status: 'installed' }));
      toast.success(t('appSettings.updater.installed'));
    } catch (e) {
      updaterStore.update((s) => ({
        ...s,
        status: 'error',
        error: e instanceof Error ? e.message : String(e),
      }));
      toast.error(t('appSettings.updater.installFailed'));
    }
  }

  onMount(async () => {
    try {
      const info = await host.checkForUpdates();
      if (info) {
        updaterStore.set({
          status: 'available',
          info,
          error: null,
          lastChecked: new Date(),
        });
        const t = get(_);
        toast.info(t('appSettings.updater.toast.title'), {
          description: t('appSettings.updater.toast.description', {
            values: { version: info.latestVersion },
          }),
          action: {
            label: t('appSettings.updater.install'),
            onClick: () => {
              void installUpdate();
            },
          },
        });
      } else {
        updaterStore.update((s) => ({
          ...s,
          status: 'up-to-date',
          lastChecked: new Date(),
        }));
      }
    } catch {
      // Silent failure on startup — don't disturb the user
      updaterStore.update((s) => ({ ...s, status: 'idle' }));
    }
  });
</script>
