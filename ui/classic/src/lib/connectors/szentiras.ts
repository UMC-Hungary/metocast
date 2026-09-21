import type { ConnectorDefinition } from './types.js';
import type { ConnectorConfigMap } from '@metocast/core-client/schemas/connectors';

export const szentirasDefinition: ConnectorDefinition<ConnectorConfigMap['szentiras']> = {
  id: 'szentiras',
  name: 'Szentírás.eu',
  category: 'platform',
  capabilities: { streaming: false, recording: false, live: false },
  infoMarkdown: `## Szentírás.eu

[Szentírás.eu](https://szentiras.eu) is a volunteer-run Hungarian Bible site. Its REST API is
what fills in Textus and Lekció verse text for the classic translations — **RUF, KG, KNB,
SZIT, BD** and **STL**.

### How it works

- **Reference lookup** — the app asks \`/api/idezet/<reference>/<translation>\` for a passage
  (e.g. \`1Kor 13,10-13\`), and gets back the verses with a "machine code" location such as
  \`JHN_3_16\` (USX book code, chapter, verse).
- **Autocomplete** — as you type a reference the app queries \`/kereses/suggest\`, which is
  public and needs no key.
- The two \`*_v2\` translations come from a different service and are unaffected by this
  connector.

### API key

Every \`/api/*\` call requires a free API key sent as an \`X-API-Key\` header.

1. Register at [szentiras.eu](https://szentiras.eu) and open
   [Profile → API keys](https://szentiras.eu/profile/api-keys).
2. Create a key and copy it.
3. Paste it below, tick **Enabled**, and save.

Without a valid key the classic translations return **401** and no verse text appears; V2
translations and autocomplete keep working.

### Limits

Roughly **60 requests per minute** per key. A disabled key returns **403**, exceeding the
limit returns **429**. The operators ask that you credit szentiras.eu wherever the text is
shown publicly, and note that the service carries no availability guarantee.`,
  isConfigured(config) {
    return config.enabled && config.apiKey.length > 0;
  },
};
