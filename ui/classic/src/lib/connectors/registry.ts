import { obsDefinition } from './obs.js';
import { vmixDefinition } from './vmix.js';
import { atemDefinition } from './atem.js';
import { middlecontrolDefinition } from './middlecontrol.js';
import { youtubeDefinition } from './youtube.js';
import { facebookDefinition } from './facebook.js';
import { discordDefinition } from './discord.js';
import { broadlinkDefinition } from './broadlink.js';
import { szentirasDefinition } from './szentiras.js';
import type { BaseConfig, ConnectorDefinition } from './types.js';

export const CONNECTORS: ConnectorDefinition<BaseConfig>[] = [
  obsDefinition as ConnectorDefinition<BaseConfig>,
  vmixDefinition as ConnectorDefinition<BaseConfig>,
  atemDefinition as ConnectorDefinition<BaseConfig>,
  middlecontrolDefinition as ConnectorDefinition<BaseConfig>,
  broadlinkDefinition as ConnectorDefinition<BaseConfig>,
  youtubeDefinition as ConnectorDefinition<BaseConfig>,
  facebookDefinition as ConnectorDefinition<BaseConfig>,
  discordDefinition as ConnectorDefinition<BaseConfig>,
  szentirasDefinition as ConnectorDefinition<BaseConfig>,
];

export function findConnector(id: string): ConnectorDefinition<BaseConfig> | undefined {
  return CONNECTORS.find((c) => c.id === id);
}
