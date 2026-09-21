<script lang="ts">
  import { locale, _ } from 'svelte-i18n';
  import { setLocale, availableLocales } from '$lib/i18n';
</script>

<section>
  <h2>{$_('appSettings.language')}</h2>
  <div class="lang-buttons">
    {#each availableLocales as lang (lang.code)}
      <button
        class="lang-btn"
        class:active={$locale === lang.code}
        onclick={() => setLocale(lang.code)}
      >
        {lang.flag}
        {lang.name}
      </button>
    {/each}
  </div>
</section>

<style>
  section {
    padding: 1.25rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  h2 {
    font-size: 1.125rem;
    margin: 0 0 0.75rem;
  }

  .lang-buttons {
    display: flex;
    gap: 0.5rem;
  }

  .lang-btn {
    padding: 0.5rem 1rem;
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .lang-btn:hover {
    background: var(--nav-item-hover);
  }

  .lang-btn.active {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
</style>
