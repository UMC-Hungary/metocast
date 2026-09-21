import type { ConnectorDefinition } from './types.js';

export interface FacebookConfig {
  enabled: boolean;
  appId: string;
  appSecret: string;
  pageId: string;
}

export const facebookDefinition: ConnectorDefinition<FacebookConfig> = {
  id: 'facebook',
  name: 'Facebook',
  category: 'platform',
  capabilities: { streaming: false, recording: false, live: true },
  infoMarkdown: `## Facebook Connection Troubleshooting

1. Go to [Facebook for Developers](https://developers.facebook.com/) and create an app.
2. Add the **Pages API** and **Live Video API** products to your app.
3. In Settings, enter your **App ID**, **App Secret**, and **Page ID**, then save.
4. Click **Login with Facebook** and authenticate.
5. Ensure the connected account has admin access to your church's Facebook Page.`,
  isConfigured(config) {
    return config.enabled && config.appId.length > 0 && config.appSecret.length > 0;
  },
};
