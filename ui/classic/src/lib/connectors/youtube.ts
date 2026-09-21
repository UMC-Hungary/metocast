import type { ConnectorDefinition } from './types.js';

export interface YouTubeConfig {
  enabled: boolean;
  clientId: string;
  clientSecret: string;
}

export const youtubeDefinition: ConnectorDefinition<YouTubeConfig> = {
  id: 'youtube',
  name: 'YouTube',
  category: 'platform',
  capabilities: { streaming: false, recording: false, live: true },
  infoMarkdown: `## YouTube Connection Troubleshooting

1. Go to the [Google Cloud Console](https://console.cloud.google.com/) and create an OAuth 2.0 Client ID.
2. Enable the **YouTube Data API v3** for your project.
3. In Settings, enter your **Client ID** and **Client Secret**, then save.
4. Click **Login with YouTube** and authenticate with your church's Google account.
5. Grant the required YouTube permissions when prompted.`,
  isConfigured(config) {
    return config.enabled && config.clientId.length > 0 && config.clientSecret.length > 0;
  },
};
