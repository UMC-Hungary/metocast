import type { ConnectorDefinition } from './types.js';

export interface BroadlinkConfig {
  enabled: boolean;
}

export const broadlinkDefinition: ConnectorDefinition<BroadlinkConfig> = {
  id: 'broadlink',
  name: 'Broadlink RF/IR',
  category: 'software-device',
  capabilities: { streaming: false, recording: false, live: false },
  isConfigured: (c) => c.enabled,
};
