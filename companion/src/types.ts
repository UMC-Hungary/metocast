import { z } from 'zod'

export interface ModuleConfig {
	host: string
	port: number
	authToken: string
	pollInterval: number
}

export interface RfIrCommand {
	id: string
	name: string
	slug: string
	deviceId: string | null
	code: string
	codeType: string
	category: 'projector' | 'screen' | 'hvac' | 'lighting' | 'audio' | 'other'
}

export interface BroadlinkDevice {
	id: string
	name: string
	type: string
	model: string
	host: string
	mac: string
}

// PPT Selector Types
export interface PptFolder {
	id: string
	path: string
	name: string
}

export interface PptFile {
	id: string
	name: string
	path: string
	folderId: string
}

export interface PptFilesResponse {
	files: PptFile[]
	total: number
	filter: string | null
}

export interface PptFoldersResponse {
	folders: PptFolder[]
}

export interface PptOpenResponse {
	success: boolean
	fileName: string
	presenterStarted: boolean
}

// Keynote Status (from backend WS)
export interface KeynoteStatus {
	appRunning: boolean
	slideshowActive: boolean
	currentSlide: number | null
	totalSlides: number | null
	documentName: string | null
}

// Presentation Control Types
export interface PresentationStatus {
	app: string | null
	appRunning: boolean
	slideshowActive: boolean
	currentSlide: number | null
	totalSlides: number | null
	currentSlideTitle: string | null
	blanked: boolean
}

export const PresentationSettingsMessageSchema = z.object({
	type: z.literal('presentation.settings'),
	useWebPresenter: z.boolean(),
	presenterTheme: z.enum(['classic', 'editorial']).default('classic'),
})

export const EventSummarySchema = z.object({
	id: z.string().uuid(),
	title: z.string(),
	dateTime: z.string(),
	speaker: z.string(),
	recordingCount: z.number().int().nonnegative(),
	isCompleted: z.boolean(),
	createdAt: z.string(),
	updatedAt: z.string(),
})

export const PresenterEventListResponseSchema = z.object({
	type: z.literal('events.presenter_list'),
	events: z.array(EventSummarySchema),
	selectedEventId: z.string().uuid().nullable(),
})

export type EventSummary = z.infer<typeof EventSummarySchema>
export type PresenterEventListResponse = z.infer<typeof PresenterEventListResponseSchema>
export type BibleReferenceType = 'textus' | 'leckio'

export const CATEGORY_COLORS: Record<string, number> = {
	projector: 0xdd614a,
	screen: 0xf48668,
	hvac: 0xf4a698,
	lighting: 0xc5c392,
	audio: 0x73a580,
	other: 0x808080,
}
