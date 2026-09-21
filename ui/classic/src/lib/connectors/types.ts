export type ConnectorCategory = 'platform' | 'software-device';
export type ConnectorStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface ConnectorCapabilities {
  readonly streaming: boolean;
  readonly recording: boolean;
  readonly live: boolean;
}

export interface ConnectorState {
  connection: ConnectorStatus;
  isStreaming?: boolean;
  isRecording?: boolean;
  isLive?: boolean;
  selectedCamera?: number | null;
  recording?: boolean | null;
  recordingCameraIds?: number[] | null;
  connectedCameraIds?: number[] | null;
  connectedApcrIds?: number[] | null;
  presetMoveActive?: boolean | null;
}

export interface BaseConfig {
  enabled: boolean;
}

/** Every connector MUST satisfy this interface. */
export interface ConnectorDefinition<TConfig extends BaseConfig> {
  readonly id: string;
  readonly name: string;
  readonly category: ConnectorCategory;
  readonly capabilities: ConnectorCapabilities;
  readonly infoMarkdown?: string;
  isConfigured(config: TConfig): boolean;
}
