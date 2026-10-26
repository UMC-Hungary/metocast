import { z } from 'zod';

/**
 * Automations: rules that run inside the core. When one device does something, Metocast does
 * something on others. A rule fires when ANY of its triggers happens and runs ALL its actions, in
 * order. See `metocast_core::automations`, which this mirrors field for field.
 */

const PresetSchema = z.number().int().min(1).max(12);
const CameraIdSchema = z.number().int().min(1).max(99);

/** Which mute counts when a channel trigger decides the channel is live. */
export const ChannelSourceSchema = z.enum(['either', 'desk', 'transmitter']);

export const AutomationTriggerSchema = z.discriminatedUnion('kind', [
  /** A Middle Control preset's move finished, wherever it was clicked. `null`: any camera. */
  z.object({
    kind: z.literal('middlecontrol.preset'),
    preset: PresetSchema,
    cameraId: CameraIdSchema.nullable(),
  }),
  /** A RØDECaster channel went live or silent and stayed so for `holdSeconds`. */
  z.object({
    kind: z.literal('rodecaster.channel'),
    /** The zero-based fader slot, as in `rodecaster.mute.set`. */
    channel: z.number().int().min(0).max(63),
    /** A snapshot of the channel's name, for display only. */
    label: z.string().max(80),
    live: z.boolean(),
    source: ChannelSourceSchema,
    holdSeconds: z.number().min(0).max(60),
  }),
]);

/** What a `record.start` or `record.stop` action drives. The names match the connectors'. */
export const RecordTargetSchema = z.discriminatedUnion('device', [
  z.object({ device: z.literal('obs') }),
  z.object({ device: z.literal('atem') }),
  z.object({ device: z.literal('blackmagic-camera') }),
  /** Cameras recorded through Middle Control. `null`: every camera. */
  z.object({ device: z.literal('middlecontrol'), cameraId: CameraIdSchema.nullable() }),
  /** The desk's own SD or USB recorder. */
  z.object({ device: z.literal('rodecaster') }),
  /** Metocast's FLAC recording of the desk's USB audio, filed under the current event. */
  z.object({ device: z.literal('rodecaster-audio') }),
]);

export const AutomationActionSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('record.start'), target: RecordTargetSchema }),
  z.object({ kind: z.literal('record.stop'), target: RecordTargetSchema }),
  /** Moves a camera to a preset. `null`: the selected camera. */
  z.object({
    kind: z.literal('middlecontrol.preset.recall'),
    preset: PresetSchema,
    cameraId: CameraIdSchema.nullable(),
    /** The actions after this one wait until the camera arrives. */
    waitForArrival: z.boolean(),
  }),
]);

/** The limits the core enforces, so an editor refuses what the server would. */
export const AUTOMATION_LIMITS = {
  maxRules: 50,
  nameMaxChars: 80,
  maxTriggers: 20,
  maxActions: 40,
  holdSeconds: { min: 0, max: 60 },
  cooldownSeconds: { min: 1, max: 600, default: 5 },
} as const;

/** A rule as a client writes it (`POST`, and `PUT`, which replaces the whole rule). */
export const AutomationDraftSchema = z.object({
  name: z.string().trim().min(1).max(AUTOMATION_LIMITS.nameMaxChars),
  enabled: z.boolean(),
  triggers: z.array(AutomationTriggerSchema).min(1).max(AUTOMATION_LIMITS.maxTriggers),
  actions: z.array(AutomationActionSchema).min(1).max(AUTOMATION_LIMITS.maxActions),
  /** Re-fires of the same rule inside this window are ignored. */
  cooldownSeconds: z
    .number()
    .min(AUTOMATION_LIMITS.cooldownSeconds.min)
    .max(AUTOMATION_LIMITS.cooldownSeconds.max),
  /** The rule fires only when the server has a current event today. */
  requireEvent: z.boolean(),
});

/** A rule as the core stores it. Lenient about limits: the core already enforced them. */
export const AutomationSchema = z.object({
  id: z.string().uuid(),
  name: z.string(),
  enabled: z.boolean(),
  triggers: z.array(AutomationTriggerSchema),
  actions: z.array(AutomationActionSchema),
  cooldownSeconds: z.number(),
  requireEvent: z.boolean(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

/** `GET /api/automations`. `armed` is the master switch: nothing fires while it is off. */
export const AutomationListSchema = z.object({
  armed: z.boolean(),
  automations: z.array(AutomationSchema),
});

export const ActionResultSchema = z.object({
  /** What the action was, in words: "Start OBS", "Move camera to preset 3". */
  action: z.string(),
  outcome: z.enum(['done', 'skipped', 'failed']),
  /** Why it was skipped, a note on a camera wait, a connector's error code, or the device's words. */
  message: z.string().nullable(),
});

export const AutomationRunSchema = z.object({
  id: z.string().uuid(),
  automationId: z.string().uuid(),
  name: z.string(),
  /** What fired it, in words: "Preset 3 finished on camera 1". */
  cause: z.string(),
  /** "Run now", which ignores the cooldown, the event check and the master switch. */
  manual: z.boolean(),
  startedAt: z.string(),
  results: z.array(ActionResultSchema),
});

/** The recorders the Record button starts and stops (`/api/settings/record-button`). */
export const RecordButtonSchema = z.object({ targets: z.array(RecordTargetSchema) });

/** One recorder as the Record button shows it; `lastStart` is the Record button's or an automation's. */
export const RecorderRowSchema = z.object({
  target: RecordTargetSchema,
  selected: z.boolean(),
  available: z.boolean(),
  recording: z.boolean(),
  lastStart: z.object({ at: z.string(), error: z.string().nullable() }).nullable(),
});

/** Server → client: the Record button's recorders, and every recording that stops. */
export const RecorderMessageSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('recorders.state'), recorders: z.array(RecorderRowSchema) }),
  z.object({
    type: z.literal('recorder.stopped'),
    target: RecordTargetSchema,
    reason: z.enum(['metocast', 'disconnected', 'device']),
    error: z.string().nullable(),
  }),
]);

/** Server → client. `automations.changed` is also the answer to `automations.list`. */
export const AutomationMessageSchema = z.discriminatedUnion('type', [
  z.object({
    type: z.literal('automations.changed'),
    armed: z.boolean(),
    automations: z.array(AutomationSchema),
  }),
  z.object({ type: z.literal('automation.run'), run: AutomationRunSchema }),
]);

export type ChannelSource = z.infer<typeof ChannelSourceSchema>;
export type AutomationTrigger = z.infer<typeof AutomationTriggerSchema>;
export type RecordTarget = z.infer<typeof RecordTargetSchema>;
export type AutomationAction = z.infer<typeof AutomationActionSchema>;
export type AutomationDraft = z.infer<typeof AutomationDraftSchema>;
export type Automation = z.infer<typeof AutomationSchema>;
export type AutomationList = z.infer<typeof AutomationListSchema>;
export type ActionResult = z.infer<typeof ActionResultSchema>;
export type AutomationRun = z.infer<typeof AutomationRunSchema>;
export type AutomationMessage = z.infer<typeof AutomationMessageSchema>;
export type RecordButton = z.infer<typeof RecordButtonSchema>;
export type RecorderRow = z.infer<typeof RecorderRowSchema>;
export type RecorderMessage = z.infer<typeof RecorderMessageSchema>;
