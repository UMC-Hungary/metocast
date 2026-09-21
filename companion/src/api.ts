import WebSocket from 'ws'
import {
	PresentationSettingsMessageSchema,
	PresenterEventListResponseSchema,
	type BibleReferenceType,
	type EventSummary,
	type ModuleConfig,
	type PptFile,
	type PptFilesResponse,
	type PptFolder,
	type PresentationStatus,
	type RfIrCommand,
} from './types.js'

type AnyResolver = (data: unknown) => void

export class MetocastApi {
	private config: ModuleConfig
	private ws: WebSocket | null = null
	private wsReconnectTimer: ReturnType<typeof setTimeout> | null = null
	private wsPingTimer: ReturnType<typeof setInterval> | null = null
	// Maps slug → UUID for broadlink commands (populated on broadlink.commands.list response)
	private commandSlugToId = new Map<string, string>()
	// Queued resolvers waiting for a specific WS response type
	private pendingRequests = new Map<string, AnyResolver[]>()

	private onConnectionChange?: (connected: boolean) => void
	private onPptFoldersChanged?: (folders: PptFolder[]) => void
	private onPptFileOpened?: (fileName: string, success: boolean, presenterStarted: boolean) => void
	private onPresentationStatusChanged?: (status: PresentationStatus) => void
	private onPresentationSettingsChanged?: (useWebPresenter: boolean) => void
	private onEventsChanged?: () => void
	private useWebPresenter = false

	constructor(config: ModuleConfig) {
		this.config = config
	}

	updateConfig(config: ModuleConfig): void {
		this.config = config
	}

	private get wsUrl(): string {
		return `ws://${this.config.host}:${this.config.port}/ws?token=${encodeURIComponent(this.config.authToken)}`
	}

	// ── WS request/response ───────────────────────────────────────────────────

	private async wsRequest<T>(
		sendType: string,
		params: Record<string, unknown> | undefined,
		responseType: string,
		timeoutMs = 8000,
	): Promise<T | null> {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return null
		return new Promise<T | null>((resolve) => {
			let settled = false

			const timer = setTimeout(() => {
				if (settled) return
				settled = true
				const queue = this.pendingRequests.get(responseType)
				if (queue) {
					const idx = queue.indexOf(handler)
					if (idx !== -1) queue.splice(idx, 1)
					if (queue.length === 0) this.pendingRequests.delete(responseType)
				}
				resolve(null)
			}, timeoutMs)

			const handler: AnyResolver = (data) => {
				if (settled) return
				settled = true
				clearTimeout(timer)
				resolve(data as T)
			}

			const queue = this.pendingRequests.get(responseType) ?? []
			queue.push(handler)
			this.pendingRequests.set(responseType, queue)

			this.sendWsCommand(sendType, params)
		})
	}

	private resolvePending(responseType: string, data: unknown): void {
		const queue = this.pendingRequests.get(responseType)
		if (!queue || queue.length === 0) return
		const resolver = queue.shift()!
		if (queue.length === 0) this.pendingRequests.delete(responseType)
		resolver(data)
	}

	private clearPendingRequests(): void {
		for (const [, queue] of this.pendingRequests) {
			for (const resolver of queue) resolver(null)
		}
		this.pendingRequests.clear()
	}

	// ── Data fetching via WS ──────────────────────────────────────────────────

	async getCommands(): Promise<RfIrCommand[]> {
		const data = await this.wsRequest<{
			commands: Array<{
				id: string
				deviceId: string | null
				name: string
				slug: string
				code: string
				codeType: string
				category: string
			}>
		}>('broadlink.commands.list', undefined, 'broadlink.commands.list')

		if (!data?.commands) return []

		this.commandSlugToId.clear()
		return data.commands.map((c) => {
			this.commandSlugToId.set(c.slug, c.id)
			return {
				id: c.id,
				name: c.name,
				slug: c.slug,
				deviceId: c.deviceId,
				code: c.code,
				codeType: c.codeType,
				category: c.category as RfIrCommand['category'],
			}
		})
	}

	async getPptFolders(): Promise<PptFolder[]> {
		const data = await this.wsRequest<{ folders: PptFolder[] }>('ppt.folders.list', undefined, 'ppt.folders.list')
		return data?.folders ?? []
	}

	async getPptFiles(filter?: string): Promise<PptFilesResponse> {
		const data = await this.wsRequest<{ files: PptFile[] }>(
			'ppt.search',
			{ filter: filter ?? '' },
			'ppt.search_results',
		)
		if (!data) return { files: [], total: 0, filter: filter ?? null }
		return { files: data.files, total: data.files.length, filter: filter ?? null }
	}

	async getPresenterEvents(): Promise<{ events: EventSummary[]; selectedEventId: string | null }> {
		const data = await this.wsRequest<unknown>('events.presenter_list', undefined, 'events.presenter_list', 5000)
		const result = PresenterEventListResponseSchema.safeParse(data)
		if (!result.success) return { events: [], selectedEventId: null }
		return {
			events: result.data.events,
			selectedEventId: result.data.selectedEventId,
		}
	}

	async presentationStatus(): Promise<PresentationStatus | null> {
		const data = await this.wsRequest<{
			status: {
				appRunning: boolean
				slideshowActive: boolean
				currentSlide: number | null
				totalSlides: number | null
				documentName: string | null
			}
		}>('keynote.status', undefined, 'keynote.status', 5000)

		if (!data?.status) return null
		const s = data.status
		return {
			app: 'keynote',
			appRunning: s.appRunning,
			slideshowActive: s.slideshowActive,
			currentSlide: s.currentSlide,
			totalSlides: s.totalSlides,
			currentSlideTitle: s.documentName,
			blanked: false,
		}
	}

	// ── Action commands (fire-and-forget via WS) ──────────────────────────────

	async executeCommand(slug: string): Promise<{ success: boolean; error?: string }> {
		const id = this.commandSlugToId.get(slug)
		if (!id) return { success: false, error: `Command not found: ${slug}` }
		const sent = this.sendWsCommand('broadlink.commands.send', { id })
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async openPptFile(filePath: string): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.open', { file_path: filePath })
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationOpen(filePath: string): Promise<{ success: boolean; error?: string }> {
		return this.openPptFile(filePath)
	}

	async presentationStart(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.start')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationStop(): Promise<{ success: boolean; error?: string }> {
		if (this.useWebPresenter) return this.presentationClose()
		const sent = this.sendWsCommand('presentation.stop')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationClose(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.close')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationCloseLatest(): Promise<{ success: boolean; error?: string }> {
		return this.presentationClose()
	}

	async presentationCloseAll(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.close_all')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationNext(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.next')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationPrevious(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.prev')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationGoto(slideNumber: number): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.goto', { slide: slideNumber })
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationFirst(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.first')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationLast(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.last')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationBlank(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.mute')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentationUnblank(): Promise<{ success: boolean; error?: string }> {
		const sent = this.sendWsCommand('presentation.unmute')
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	async presentBibleReference(
		referenceType: BibleReferenceType,
		eventId?: string,
	): Promise<{ success: boolean; error?: string }> {
		const payload: Record<string, unknown> = { reference_type: referenceType }
		if (eventId) payload['event_id'] = eventId
		const sent = this.sendWsCommand('presenter.load_bible_reference', payload)
		return sent ? { success: true } : { success: false, error: 'WebSocket not connected' }
	}

	// ── WebSocket management ──────────────────────────────────────────────────

	/** Send a command over the active WebSocket. Returns true if sent. */
	sendWsCommand(type: string, data?: Record<string, unknown>): boolean {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return false
		try {
			this.ws.send(JSON.stringify({ type, ...data }))
			return true
		} catch {
			return false
		}
	}

	setCallbacks(callbacks: {
		onConnectionChange?: (connected: boolean) => void
		onPptFoldersChanged?: (folders: PptFolder[]) => void
		onPptFileOpened?: (fileName: string, success: boolean, presenterStarted: boolean) => void
		onPresentationStatusChanged?: (status: PresentationStatus) => void
		onPresentationSettingsChanged?: (useWebPresenter: boolean) => void
		onEventsChanged?: () => void
	}): void {
		this.onConnectionChange = callbacks.onConnectionChange
		this.onPptFoldersChanged = callbacks.onPptFoldersChanged
		this.onPptFileOpened = callbacks.onPptFileOpened
		this.onPresentationStatusChanged = callbacks.onPresentationStatusChanged
		this.onPresentationSettingsChanged = callbacks.onPresentationSettingsChanged
		this.onEventsChanged = callbacks.onEventsChanged
	}

	connectWebSocket(): void {
		if (this.ws) this.disconnectWebSocket()

		try {
			this.ws = new WebSocket(this.wsUrl)

			this.ws.on('open', () => {
				this.onConnectionChange?.(true)
				this.startPingInterval()
			})

			this.ws.on('close', () => {
				this.stopPingInterval()
				this.clearPendingRequests()
				this.onConnectionChange?.(false)
				this.scheduleReconnect()
			})

			this.ws.on('error', (error) => {
				console.error('WebSocket error:', error)
			})

			this.ws.on('message', (raw) => {
				try {
					const message = JSON.parse(raw.toString()) as { type: string; [key: string]: unknown }
					this.handleMessage(message)
				} catch {
					// Ignore parse errors
				}
			})
		} catch (error) {
			console.error('Failed to connect WebSocket:', error)
			this.scheduleReconnect()
		}
	}

	private handleMessage(message: { type: string; [key: string]: unknown }): void {
		switch (message.type) {
			case 'presentation.settings': {
				const result = PresentationSettingsMessageSchema.safeParse(message)
				if (result.success) {
					this.useWebPresenter = result.data.useWebPresenter
					this.onPresentationSettingsChanged?.(result.data.useWebPresenter)
				}
				break
			}
			case 'presentation.status': {
				const s = message.status as
					| {
							appRunning?: boolean
							slideshowActive?: boolean
							currentSlide?: number | null
							totalSlides?: number | null
							documentName?: string | null
							blanked?: boolean
					  }
					| undefined
				if (s) {
					this.onPresentationStatusChanged?.({
						app: this.useWebPresenter ? 'web' : 'keynote',
						appRunning: s.appRunning ?? false,
						slideshowActive: s.slideshowActive ?? false,
						currentSlide: s.currentSlide ?? null,
						totalSlides: s.totalSlides ?? null,
						currentSlideTitle: s.documentName ?? null,
						blanked: s.blanked ?? false,
					})
				}
				break
			}
			case 'broadlink.commands.list': {
				this.resolvePending('broadlink.commands.list', message)
				break
			}
			case 'ppt.folders.list': {
				// Only resolves pending getPptFolders() calls; folder-change broadcasts are
				// handled by ppt.folders_changed which fetches and calls onPptFoldersChanged.
				this.resolvePending('ppt.folders.list', message)
				break
			}
			case 'ppt.search_results': {
				this.resolvePending('ppt.search_results', message)
				break
			}
			case 'events.presenter_list': {
				this.resolvePending('events.presenter_list', message)
				break
			}
			case 'event.changed': {
				this.onEventsChanged?.()
				break
			}
			case 'ppt.folders_changed': {
				// Server broadcast: folder list changed — fetch fresh list and notify
				void this.getPptFolders().then((folders) => {
					this.onPptFoldersChanged?.(folders)
				})
				break
			}
			case 'notification': {
				const level = message.level as string | undefined
				const msg = message.message as string | undefined
				if (level === 'warn') console.warn('[Metocast]', msg)
				else console.info('[Metocast]', msg)
				break
			}
			case 'error': {
				const errMsg = message.message as string | undefined
				if (errMsg === 'unauthorized') {
					console.error(
						'[Metocast] WebSocket auth failed — check the Auth Token in module settings matches the token shown in the Metocast app.',
					)
					this.onConnectionChange?.(false)
				} else {
					console.error('[Metocast] Server error:', errMsg)
				}
				break
			}
			default:
				break
		}
	}

	private scheduleReconnect(): void {
		if (this.wsReconnectTimer) clearTimeout(this.wsReconnectTimer)
		this.wsReconnectTimer = setTimeout(() => {
			this.connectWebSocket()
		}, 5000)
	}

	private startPingInterval(): void {
		this.stopPingInterval()
		this.wsPingTimer = setInterval(() => {
			if (this.ws?.readyState === WebSocket.OPEN) {
				try {
					this.ws.send(JSON.stringify({ type: 'ping' }))
				} catch {
					// Ignore send errors; close event will handle reconnect
				}
			}
		}, 15000)
	}

	private stopPingInterval(): void {
		if (this.wsPingTimer) {
			clearInterval(this.wsPingTimer)
			this.wsPingTimer = null
		}
	}

	disconnectWebSocket(): void {
		this.stopPingInterval()
		if (this.wsReconnectTimer) {
			clearTimeout(this.wsReconnectTimer)
			this.wsReconnectTimer = null
		}
		this.clearPendingRequests()
		if (this.ws) {
			this.ws.removeAllListeners()
			this.ws.close()
			this.ws = null
		}
	}
}
