import type { ConnectorDefinition } from './types.js';

export interface MiddlecontrolConfig {
  enabled: boolean;
  host: string;
  port: number;
}

export const middlecontrolDefinition: ConnectorDefinition<MiddlecontrolConfig> = {
  id: 'middlecontrol',
  name: 'Middle Control',
  category: 'software-device',
  capabilities: { streaming: false, recording: true, live: false },
  infoMarkdown: `## Middle Control

Middle Control must remain running. Current macOS versions use port 11584; the observed 3.2.0 macOS build uses 11581; Windows uses 11580.`,
  isConfigured(config) {
    return config.enabled && config.host.length > 0;
  },
};
