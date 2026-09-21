import type { ConnectorDefinition } from './types.js';

export interface ObsConfig {
  enabled: boolean;
  host: string;
  port: number;
  password: string | null;
}

export const obsDefinition: ConnectorDefinition<ObsConfig> = {
  id: 'obs',
  name: 'OBS Studio',
  category: 'software-device',
  capabilities: { streaming: true, recording: true, live: false },
  infoMarkdown: `## OBS Studio Connection Troubleshooting

1. Open OBS Studio on this machine.
2. Go to **Tools → WebSocket Server Settings** and enable the WebSocket server.
3. Note the port (default: 4455) and set a secure password.
4. Click **Apply** and **OK**.
5. In Settings, enter the host, port, and password, then click **Save & Reconnect**.`,
  isConfigured(config) {
    return config.enabled && config.host.length > 0 && config.port > 0;
  },
};
