import type { ConnectorDefinition } from './types.js';

export interface AtemConfig {
  enabled: boolean;
  host: string;
  port: number;
}

export const atemDefinition: ConnectorDefinition<AtemConfig> = {
  id: 'atem',
  name: 'Blackmagic ATEM',
  category: 'software-device',
  capabilities: { streaming: true, recording: true, live: false },
  infoMarkdown: `## Blackmagic ATEM Connection Troubleshooting

1. Ensure your ATEM switcher is connected to the same network as the server.
2. Note the ATEM IP address from ATEM Setup or ATEM Software Control.
3. Configure and control the switcher from the Sanctum UI: Settings → Connectors → Blackmagic ATEM (default port 9910).`,
  isConfigured(config) {
    return config.enabled && config.host.length > 0;
  },
};
