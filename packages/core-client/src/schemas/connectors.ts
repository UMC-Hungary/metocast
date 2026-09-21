import { z } from 'zod';

/**
 * A credential the core never returns. Reads come back blank with a companion
 * `<field>Set` flag telling the UI whether one is stored; writing a blank value
 * keeps whatever the server already has.
 */
const secretSet = z.boolean().optional();

export const ObsConfigSchema = z.object({
  enabled: z.boolean(),
  host: z.string(),
  port: z.number(),
  password: z.string().nullable(),
  passwordSet: secretSet,
});

export const VmixConfigSchema = z.object({
  enabled: z.boolean(),
  host: z.string(),
  port: z.number(),
});

export const AtemConfigSchema = z.object({
  enabled: z.boolean(),
  host: z.string(),
  port: z.number().int().min(1).max(65535),
});

export const MiddlecontrolConfigSchema = z.object({
  enabled: z.boolean(),
  host: z.string(),
  port: z.number().int().min(1).max(65535),
});

/** Blackmagic camera. A blank `fingerprint` means trust-on-first-use. */
export const BlackmagicCameraConfigSchema = z.object({
  enabled: z.boolean(),
  host: z.string(),
  fingerprint: z.string(),
  username: z.string(),
  password: z.string(),
  passwordSet: secretSet,
});

export const BroadlinkConfigSchema = z.object({
  enabled: z.boolean(),
});

export const RodecasterAudioSourceSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('mainMix') }).strict(),
  z.object({ kind: z.literal('faderSlot'), number: z.number().int().positive() }).strict(),
]);

export const RodecasterAudioRecordingConfigSchema = z
  .object({
    schemaVersion: z.literal(1),
    enabled: z.boolean(),
    directory: z.string(),
    processingMode: z.enum(['preFader', 'preFaderBypass', 'postFader']),
    outputMode: z.enum(['mainMix', 'separate', 'combinedStereo', 'multichannelFlac']),
    sources: z.array(RodecasterAudioSourceSchema),
  })
  .strict();

/** RØDECaster Pro II. Found by USB id, so there is nothing to address it by. */
export const RodecasterConfigSchema = z
  .object({
    enabled: z.boolean(),
    notifyOnMute: z.boolean(),
    audioRecording: RodecasterAudioRecordingConfigSchema,
  })
  .strict();

export const YouTubeConfigSchema = z.object({
  enabled: z.boolean(),
  clientId: z.string(),
  clientSecret: z.string(),
  clientSecretSet: secretSet,
});

export const FacebookConfigSchema = z.object({
  enabled: z.boolean(),
  appId: z.string(),
  appSecret: z.string(),
  appSecretSet: secretSet,
  pageId: z.string(),
});

export const DiscordConfigSchema = z.object({
  enabled: z.boolean(),
  webhookUrl: z.string(),
  webhookUrlSet: secretSet,
});

export const SzentirasConfigSchema = z.object({
  enabled: z.boolean(),
  apiKey: z.string(),
  apiKeySet: secretSet,
});

/** Every connector whose config the core stores, keyed by its API path segment. */
export const ConnectorConfigSchemas = {
  obs: ObsConfigSchema,
  vmix: VmixConfigSchema,
  atem: AtemConfigSchema,
  middlecontrol: MiddlecontrolConfigSchema,
  broadlink: BroadlinkConfigSchema,
  'blackmagic-camera': BlackmagicCameraConfigSchema,
  rodecaster: RodecasterConfigSchema,
  youtube: YouTubeConfigSchema,
  facebook: FacebookConfigSchema,
  discord: DiscordConfigSchema,
  szentiras: SzentirasConfigSchema,
} as const;

export type ConnectorName = keyof typeof ConnectorConfigSchemas;
export type ConnectorConfigMap = {
  [K in ConnectorName]: z.infer<(typeof ConnectorConfigSchemas)[K]>;
};

export const MiddlecontrolStateSchema = z.object({
  selectedCamera: z.number().int().min(1).max(99).nullable(),
  recording: z.boolean().nullable(),
  recordingCameraIds: z.array(z.number().int().min(1).max(99)).nullable(),
  connectedCameraIds: z.array(z.number().int().min(1).max(99)).nullable(),
  connectedApcrIds: z.array(z.number().int().min(1).max(99)).nullable(),
  presetMoveActive: z.boolean().nullable(),
});

export type MiddlecontrolState = z.infer<typeof MiddlecontrolStateSchema>;

export const DiscoveredMiddlecontrolSchema = z.object({
  host: z.string().min(1),
  port: z.number().int().min(1).max(65535),
});

export const DiscoveredMiddlecontrolsSchema = z.object({
  devices: z.array(DiscoveredMiddlecontrolSchema),
});

export type DiscoveredMiddlecontrol = z.infer<typeof DiscoveredMiddlecontrolSchema>;

export const AtemStateSchema = z.object({
  product: z.string(),
  program: z.number().int().nullable(),
  preview: z.number().int().nullable(),
  inputs: z.array(z.object({ id: z.number().int(), name: z.string(), shortName: z.string() })),
  streaming: z.enum(['idle', 'connecting', 'streaming', 'stopping']).nullable(),
  recording: z.enum(['idle', 'recording', 'stopping']).nullable(),
  streamService: z.string().nullable(),
});

export type AtemState = z.infer<typeof AtemStateSchema>;

export const DiscoveredAtemSchema = z.object({
  name: z.string(),
  host: z.string().min(1),
  port: z.number().int().min(1).max(65535),
  usb: z.boolean(),
});

export const DiscoveredAtemsSchema = z.object({ devices: z.array(DiscoveredAtemSchema) });

export type DiscoveredAtem = z.infer<typeof DiscoveredAtemSchema>;

export const AtemStreamTargetSchema = z.object({ service: z.string(), url: z.string() });

export type AtemStreamTarget = z.infer<typeof AtemStreamTargetSchema>;

/** One camera returned by an mDNS scan. `host` is what the config field takes. */
export const DiscoveredCameraSchema = z.object({
  host: z.string(),
  hostname: z.string(),
  addresses: z.array(z.string()),
  port: z.number().int(),
  deviceName: z.string(),
  productName: z.string(),
  uniqueId: z.string(),
  softwareVersion: z.string(),
});

export const DiscoveredCamerasSchema = z.object({ cameras: z.array(DiscoveredCameraSchema) });

export type DiscoveredCamera = z.infer<typeof DiscoveredCameraSchema>;

/** Where the camera's livestream now points, after YouTube settings were pushed. */
export const CameraStreamTargetSchema = z.object({
  rtmpUrl: z.string(),
  platform: z.string(),
  server: z.string(),
  quality: z.string(),
  url: z.string().nullable(),
});

export type CameraStreamTarget = z.infer<typeof CameraStreamTargetSchema>;

/** The camera's own payloads, forwarded by the core for the camera control screen. */
const ResolutionSchema = z.object({ width: z.number(), height: z.number() });

const ResolutionDescriptorSchema = z.object({
  group: z.string(),
  aspectRatio: z.string(),
  description: z.string(),
  sensorArea: z.string().default(''),
});

export const CameraFormatSchema = z.object({
  codec: z.string(),
  frameRate: z.string(),
  recordResolution: ResolutionSchema,
  sensorResolution: ResolutionSchema,
  resolutionDescriptor: ResolutionDescriptorSchema,
  offSpeedEnabled: z.boolean().default(false),
  offSpeedFrameRate: z.number().default(0),
});

const SupportedFormatSchema = z.object({
  codecs: z.array(z.string()),
  frameRates: z.array(z.string()),
  recordResolution: ResolutionSchema,
  sensorResolution: ResolutionSchema,
  resolutionDescriptor: ResolutionDescriptorSchema,
});

/** A card or drive in the camera's working set. Only `deviceName` is guaranteed. */
const MediaDeviceSchema = z.object({
  deviceName: z.string(),
  index: z.number().default(0),
  activeDisk: z.boolean().default(false),
  volume: z.string().default(''),
  clipCount: z.number().default(0),
  remainingRecordTime: z.number().default(0),
  remainingSpace: z.number().default(0),
  totalSpace: z.number().default(0),
});

/** Where the livestream points now: the platform entry plus the stream key. */
export const CameraPlatformSchema = z.object({
  platform: z.string(),
  server: z.string(),
  quality: z.string(),
  key: z.string().nullable().default(null),
  passphrase: z.string().nullable().default(null),
  url: z.string().nullable().default(null),
});

const PlatformProfileSchema = z.object({
  profile: z.string(),
  lowLatency: z.boolean().default(false),
  configs: z
    .array(
      z.object({
        resolution: z.string(),
        fps: z.string(),
        bitrate: z.number(),
        audioBitrate: z.number().default(0),
        keyFrameInterval: z.number().default(0),
        videoCodecs: z.array(z.string()).default([]),
      }),
    )
    .default([]),
});

const PlatformServiceSchema = z.object({
  platform: z.string(),
  servers: z.array(
    z.object({ server: z.string(), url: z.string(), group: z.string().default('') }),
  ),
  profiles: z.array(PlatformProfileSchema),
  defaultProfile: z.string().nullable().default(null),
  customizableUrlEnabled: z.boolean().default(false),
});

export const CameraSettingsSchema = z.object({
  recording: z.boolean(),
  record: z.object({
    format: CameraFormatSchema,
    supported: z.object({ supportedFormats: z.array(SupportedFormatSchema) }),
  }),
  storage: z.object({
    slots: z.array(z.object({ index: z.number(), type: z.string() })),
    workingset: z.object({ size: z.number(), workingset: z.array(MediaDeviceSchema.nullable()) }),
    active: z.object({ workingsetIndex: z.number(), deviceName: z.string() }).nullable(),
  }),
  stream: z.object({
    status: z.object({
      status: z.string(),
      bitrate: z.number().default(0),
      effectiveVideoFormat: z.string().default(''),
      duration: z.number().default(0),
      cache: z.number().default(0),
    }),
    available: z.object({ available: z.boolean(), reasons: z.array(z.string()) }),
    platforms: z.array(z.string()),
    active: CameraPlatformSchema,
    platform: PlatformServiceSchema,
  }),
});

/**
 * What the control screen writes back. The camera validates a record format as a
 * whole, so all four record fields travel together or not at all.
 */
export const CameraSettingsUpdateSchema = z.object({
  record: z
    .object({
      codec: z.string(),
      frameRate: z.string(),
      recordResolution: ResolutionSchema,
      sensorResolution: ResolutionSchema,
    })
    .optional(),
  stream: CameraPlatformSchema.optional(),
});

export type CameraSettings = z.infer<typeof CameraSettingsSchema>;
export type CameraFormat = z.infer<typeof CameraFormatSchema>;
export type CameraSupportedFormat = z.infer<typeof SupportedFormatSchema>;
export type CameraMediaDevice = z.infer<typeof MediaDeviceSchema>;
export type CameraPlatform = z.infer<typeof CameraPlatformSchema>;
export type CameraPlatformProfile = z.infer<typeof PlatformProfileSchema>;
export type CameraSettingsUpdate = z.infer<typeof CameraSettingsUpdateSchema>;

/**
 * One channel of the desk, as the device reports it. `processing` names the blocks
 * it has switched on and `settings` the rest of the channel's and its input's
 * properties — both carry the device's own property names, so nothing is a guess.
 */
export const RodecasterChannelSchema = z.object({
  channel: z.number().int(),
  label: z.string(),
  source: z.number().int().nullable(),
  level: z.number(),
  mute: z.boolean(),
  wirelessMute: z.boolean(),
  cue: z.boolean(),
  processing: z.array(z.string()),
  settings: z.array(z.object({ name: z.string(), value: z.string() })),
});

export const RodecasterProfileSchema = z.object({
  model: z.string(),
  firmware: z.string(),
  channels: z.array(RodecasterChannelSchema),
});

/** A mute the device reported. `remote` distinguishes the wireless transmitter from the mixer. */
export const RodecasterMuteEventSchema = z.object({
  channel: z.number().int(),
  label: z.string(),
  muted: z.boolean(),
  remote: z.boolean(),
  notify: z.boolean(),
});

export const RodecasterAudioSourceMappingSchema = z
  .object({
    identity: RodecasterAudioSourceSchema,
    label: z.string(),
    layout: z.literal('stereo'),
    hostChannels: z.array(z.number().int().nonnegative()),
  })
  .strict();

export const RodecasterAudioEndpointSchema = z
  .object({
    id: z.string(),
    name: z.string(),
    model: z.string(),
    firmware: z.string(),
    sampleRate: z.number().int().positive(),
    channels: z.number().int().positive(),
    sampleFormat: z.string(),
    sources: z.array(RodecasterAudioSourceMappingSchema),
  })
  .strict();

export const RodecasterAudioDiscoverySchema = z
  .object({
    endpoint: RodecasterAudioEndpointSchema.nullable(),
    unavailableReason: z.string().nullable(),
  })
  .strict();

export const RodecasterFinalizedAudioFileSchema = z
  .object({
    path: z.string(),
    fileName: z.string(),
    fileSize: z.number().int().nonnegative(),
    durationSeconds: z.number().nonnegative(),
    mediaKind: z.string(),
    source: z.literal('rodecaster'),
    metadata: z.record(z.string(), z.unknown()),
  })
  .strict();

export const RodecasterAudioRecorderStateSchema = z
  .object({
    status: z.enum(['idle', 'recording', 'failed']),
    sessionId: z.string().uuid().nullable(),
    eventId: z.string().uuid().nullable(),
    outputMode: z.enum(['mainMix', 'separate', 'combinedStereo', 'multichannelFlac']).nullable(),
    startedAt: z.string().nullable(),
    endpoint: RodecasterAudioEndpointSchema.nullable(),
    error: z.string().nullable(),
    partialFiles: z.array(z.string()),
    finalizedFiles: z.array(RodecasterFinalizedAudioFileSchema),
  })
  .strict();

export type RodecasterChannel = z.infer<typeof RodecasterChannelSchema>;
export type RodecasterProfile = z.infer<typeof RodecasterProfileSchema>;
export type RodecasterMuteEvent = z.infer<typeof RodecasterMuteEventSchema>;
export type RodecasterAudioSource = z.infer<typeof RodecasterAudioSourceSchema>;
export type RodecasterAudioRecordingConfig = z.infer<typeof RodecasterAudioRecordingConfigSchema>;
export type RodecasterAudioDiscovery = z.infer<typeof RodecasterAudioDiscoverySchema>;
export type RodecasterAudioRecorderState = z.infer<typeof RodecasterAudioRecorderStateSchema>;

export const ConnectorStatusPayloadSchema = z.object({
  type: z.enum(['disconnected', 'connecting', 'connected', 'error']),
  message: z.string().optional(),
});

export const ConnectorStatusesSchema = z.record(
  z.enum(Object.keys(ConnectorConfigSchemas) as [ConnectorName, ...ConnectorName[]]),
  ConnectorStatusPayloadSchema,
);

export const ObsStreamSettingsSchema = z.object({
  serviceType: z.string(),
  server: z.string(),
  key: z.string(),
});

export type ObsStreamSettings = z.infer<typeof ObsStreamSettingsSchema>;
