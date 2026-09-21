import type { ConnectorDefinition } from './types.js';

export interface DiscordConfig {
  enabled: boolean;
  webhookUrl: string;
}

export const discordDefinition: ConnectorDefinition<DiscordConfig> = {
  id: 'discord',
  name: 'Discord',
  category: 'platform',
  capabilities: { streaming: false, recording: false, live: true },
  infoMarkdown: `## Discord Connection Troubleshooting

Discord integration is not yet fully implemented.

1. In your Discord server, go to **Server Settings → Integrations → Webhooks**.
2. Create a new webhook for the channel where announcements should be posted.
3. Copy the **Webhook URL**.
4. In Settings, paste the webhook URL and save.`,
  isConfigured(config) {
    return config.enabled && config.webhookUrl.length > 0;
  },
};
