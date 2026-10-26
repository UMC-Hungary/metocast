/**
 * Core client SDK — the only boundary a rendering UI uses to reach the core.
 *
 * A UI imports from `./core-client` and nothing else transport-shaped: no
 * `@tauri-apps/*`, no raw `fetch`, no `new WebSocket`. That rule is enforced by
 * ESLint (see `eslint.config.js`), which allows those only inside this SDK.
 *
 * Three layers live behind it:
 *
 * - **HTTP** (`./api`) — request/response core operations. Every response is
 *   validated with Zod, and the exported types are `z.infer`red from those
 *   schemas, so what the compiler believes matches what the server sent.
 * - **WebSocket** (`./ws`) — realtime events and commands, same schemas.
 * - **Host** (`./host`) — the optional desktop shell: which core this window
 *   talks to, log files, updates, native dialogs. Every capability is
 *   feature-detected via `hostCapabilities`; a UI outside the desktop app must
 *   degrade rather than assume.
 *
 * Transport selection is not a decision the UI makes. HTTP and WS always address
 * the configured core — the local one in server mode, a remote one in client
 * mode — and the host layer is a local fast-path for things that are not core
 * operations at all. Nothing here is only reachable through IPC, which is what
 * lets a UI run against a headless core.
 */

// ── Core operations over HTTP ─────────────────────────────────────────────────
export * from './api/activities.js';
export * from './api/automations.js';
export * from './api/broadlink.js';
export * from './api/client.js';
export * from './api/connectors.js';
export * from './api/cron-jobs.js';
export * from './api/events.js';
export * from './api/logs.js';
export * from './api/media.js';
export * from './api/presentations.js';
export * from './api/queues.js';
export * from './api/recordings.js';
export * from './api/settings.js';
export * from './api/transcription.js';
export * from './api/untracked-recordings.js';
export * from './api/uploads.js';
export { bibleApi } from './utils/bible-api.js';
export * from './utils/title-template.js';
export * from './utils/presenter-install.js';

// ── Realtime ──────────────────────────────────────────────────────────────────
// The transport is framework-agnostic: it validates and emits typed messages via
// handlers. A UI binds those to its own stores and notifications (see each UI).
export {
  connectWs,
  disconnectWs,
  sendWsCommand,
  connectPresenterWs,
  connectTranscriptWs,
} from './ws/client.js';
export type { WsHandlers, WsStatus } from './ws/client.js';

// ── Startup configuration ─────────────────────────────────────────────────────
// The UI supplies the core's location and token once; every layer reads it here.
export { configureCoreClient } from './config.js';
export type { CoreClientConfig, AppMode } from './config.js';

// ── Optional desktop host ─────────────────────────────────────────────────────
// Exported both flat and as a namespace: `hostCapabilities.logs` reads well at a
// call site, and `host.openExternal(...)` reads well when a component uses several.
export * from './host/index.js';
export * as host from './host/index.js';

// ── This build's rendering UIs ────────────────────────────────────────────────
export * from './uis.js';

// ── Boundary types, inferred from the Zod schemas ─────────────────────────────
export type {
  AtemState,
  AtemStreamTarget,
  DiscoveredAtem,
  CameraFormat,
  CameraMediaDevice,
  CameraPlatform,
  CameraPlatformProfile,
  CameraSettings,
  CameraSettingsUpdate,
  CameraStreamTarget,
  CameraSupportedFormat,
  ConnectorConfigMap,
  ConnectorName,
  DiscoveredCamera,
  DiscoveredMiddlecontrol,
  MiddlecontrolState,
  ObsStreamSettings,
  OpenAiConfig,
  McpConfig,
  RodecasterChannel,
  RodecasterMuteEvent,
  RodecasterProfile,
  RodecasterAudioSource,
  RodecasterAudioRecordingConfig,
  RodecasterAudioDiscovery,
  RodecasterAudioRecorderState,
  RodecasterDeskRecorder,
  LastPreset,
} from './schemas/connectors.js';
export { AUTOMATION_LIMITS, AutomationDraftSchema } from './schemas/automations.js';
export type {
  ActionResult,
  Automation,
  AutomationAction,
  AutomationDraft,
  AutomationList,
  AutomationRun,
  AutomationTrigger,
  ChannelSource,
  RecordButton,
  RecorderRow,
  RecordTarget,
} from './schemas/automations.js';
export {
  DiscoveredMiddlecontrolSchema,
  DiscoveredMiddlecontrolsSchema,
  MiddlecontrolConfigSchema,
  MiddlecontrolStateSchema,
} from './schemas/connectors.js';
export type { BiblePassage, BibleSuggestion } from './schemas/bible.js';
export type {
  AudioInfo,
  AudioProbe,
  AudioProgress,
  AudioReport,
  AudioSettings,
  AudioSource,
  ClockEstimate,
  CropRect,
  MediaProbe,
  PictureMode,
  PreviewFrame,
  ProgramProbe,
  ProgramProgress,
  ProgramReport,
  RawOptions,
  RenderProgress,
  RenderQueued,
  RenderSettings,
  SubtitleReport,
  SubtitleSource,
  SyncMethod,
  SyncReport,
  SyncWindow,
  VideoSound,
} from './schemas/media.js';
export { LOW_CREDIT_USD } from './schemas/transcription.js';
export type {
  CaptionsJobState,
  CleanupProgress,
  CleanupStatus,
  StoredSegment,
  StoredTranscript,
  TranscriptDetail,
  CreditStatus,
  OpenAiVerifyResult,
  TranscriptMessage,
  TranscriptSegment,
  TranscriptionState,
  TranscriptionStatus,
} from './schemas/transcription.js';
export { EventOrderSchema, OrderItemSchema, OrderPositionSchema } from './schemas/event.js';
export type { EventOrder, OrderItem, OrderItemKind, OrderPosition } from './schemas/event.js';
export { MiddlecontrolCommandSchema } from './schemas/ws-messages.js';
export type { MiddlecontrolCommand, WsMessage } from './schemas/ws-messages.js';
