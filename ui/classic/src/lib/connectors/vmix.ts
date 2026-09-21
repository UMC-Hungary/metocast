import type { ConnectorDefinition } from './types.js';

export interface VmixConfig {
  enabled: boolean;
  host: string;
  port: number;
}

export const vmixDefinition: ConnectorDefinition<VmixConfig> = {
  id: 'vmix',
  name: 'VMix',
  category: 'software-device',
  capabilities: { streaming: true, recording: true, live: false },
  infoMarkdown: `## VMix Connection Troubleshooting

VMix integration is not yet fully implemented.

1. Ensure VMix is running on the target machine.
2. In Settings, enter the host and port (default: 8088).
3. Save the configuration for future use when VMix support is complete.`,
  isConfigured(config) {
    return config.enabled && config.host.length > 0;
  },
};
