import { InstanceBase, InstanceStatus, runEntrypoint, type SomeCompanionConfigField } from '@companion-module/base'
import { GetConfigFields, GetDefaultConfig } from './config.js'
import { GetActions } from './actions.js'
import { GetFeedbacks } from './feedbacks.js'
import { GetPresets } from './presets.js'
import { GetVariableDefinitions, GetDefaultVariableValues } from './variables.js'
import { MetocastApi } from './api.js'
import { PptSelector } from './ppt-selector.js'
import type { EventSummary, ModuleConfig, PresentationStatus, RfIrCommand } from './types.js'

export class ModuleInstance extends InstanceBase<ModuleConfig> {
	public config: ModuleConfig = GetDefaultConfig()
	public api: MetocastApi
	public commands: RfIrCommand[] = []
	public isConnected = false
	public pptSelector: PptSelector
	public presentationStatus: PresentationStatus | null = null
	public useWebPresenter = false
	public presenterEvents: EventSummary[] = []
	public selectedPresenterEventId: string | null = null

	private pollTimer: ReturnType<typeof setInterval> | null = null
	private presentationPollTimer: ReturnType<typeof setInterval> | null = null

	constructor(internal: unknown) {
		super(internal as ConstructorParameters<typeof InstanceBase>[0])
		this.api = new MetocastApi(this.config)
		this.pptSelector = new PptSelector(this.api)
	}

	async init(config: ModuleConfig): Promise<void> {
		this.config = config
		this.api.updateConfig(config)

		this.pptSelector.setOnStateChange((state) => {
			this.setVariableValues({
				ppt_filter: state.currentFilter,
				ppt_match_count: state.matchingFiles.length.toString(),
				ppt_folder_name: this.pptSelector.getSelectedFolderName(),
				ppt_slot_1_name: this.pptSelector.getSlotDisplayName(0),
				ppt_slot_2_name: this.pptSelector.getSlotDisplayName(1),
				ppt_slot_3_name: this.pptSelector.getSlotDisplayName(2),
				ppt_slot_4_name: this.pptSelector.getSlotDisplayName(3),
				ppt_slot_5_name: this.pptSelector.getSlotDisplayName(4),
			})
			this.checkFeedbacks('ppt_slot_has_file', 'ppt_filter_active')
		})

		this.api.setCallbacks({
			onConnectionChange: (connected) => {
				this.isConnected = connected
				this.updateStatus(connected ? InstanceStatus.Ok : InstanceStatus.Disconnected)
				this.setVariableValues({ connection_status: connected ? 'Connected' : 'Disconnected' })
				this.checkFeedbacks('connection_status')

				if (connected) {
					void this.refreshCommands()
					void this.refreshPresenterEvents()
					void this.pptSelector.refreshFolders()
					this.api.sendWsCommand('presentation.get_settings')
					this.api.sendWsCommand('presentation.status')
				}
			},
			onPptFoldersChanged: (folders) => {
				this.log('debug', `PPT folders changed via WS: ${folders.length} folders`)
				this.pptSelector.updateFolders(folders)
				this.updateDefinitions()
			},
			onPptFileOpened: (fileName, success, presenterStarted) => {
				this.log('info', `PPT file opened: ${fileName} (success: ${success}, presenter: ${presenterStarted})`)
				if (success) {
					this.setVariableValues({ ppt_last_opened: fileName })
				}
			},
			onPresentationStatusChanged: (status) => {
				this.log(
					'debug',
					`Presentation status: ${status.app} slideshow=${status.slideshowActive} slide=${status.currentSlide}/${status.totalSlides}`,
				)
				this.presentationStatus = status
				this.setVariableValues({
					ppt_current_slide: status.currentSlide?.toString() ?? '-',
					ppt_total_slides: status.totalSlides?.toString() ?? '-',
					ppt_slideshow_active: status.slideshowActive ? 'ON' : 'OFF',
					ppt_app: this.useWebPresenter ? 'Web Presenter' : (status.app ?? 'None'),
					ppt_blanked: status.blanked ? 'YES' : 'NO',
					ppt_document: status.currentSlideTitle ?? '',
				})
				this.checkFeedbacks('slideshow_active', 'presentation_blanked')
			},
			onPresentationSettingsChanged: (useWebPresenter) => {
				this.useWebPresenter = useWebPresenter
				this.setVariableValues({
					ppt_app: useWebPresenter ? 'Web Presenter' : 'Keynote',
					ppt_backend: useWebPresenter ? 'Web Presenter' : 'Keynote',
				})
				this.updateDefinitions()
			},
			onEventsChanged: () => {
				void this.refreshPresenterEvents()
			},
		})

		this.updateStatus(InstanceStatus.Connecting)
		this.api.connectWebSocket()
		this.startPolling()
		this.startPresentationPolling()
		this.updateDefinitions()
	}

	async destroy(): Promise<void> {
		this.stopPolling()
		this.stopPresentationPolling()
		this.api.disconnectWebSocket()
	}

	async configUpdated(config: ModuleConfig): Promise<void> {
		const hostChanged = this.config.host !== config.host || this.config.port !== config.port
		this.config = config
		this.api.updateConfig(config)

		if (hostChanged) {
			this.api.disconnectWebSocket()
			this.updateStatus(InstanceStatus.Connecting)
			this.api.connectWebSocket()
		}

		this.stopPolling()
		this.stopPresentationPolling()
		this.startPolling()
		this.startPresentationPolling()
		this.updateDefinitions()
	}

	getConfigFields(): SomeCompanionConfigField[] {
		return GetConfigFields()
	}

	public async refreshCommands(): Promise<void> {
		const commands = await this.api.getCommands()
		this.commands = commands
		this.setVariableValues({ command_count: commands.length })
		this.updateDefinitions()
		this.log('info', `Loaded ${commands.length} commands`)
	}

	public async refreshPresenterEvents(): Promise<void> {
		const result = await this.api.getPresenterEvents()
		this.presenterEvents = result.events
		this.selectedPresenterEventId = result.selectedEventId
		const selected = this.presenterEvents.find((event) => event.id === this.selectedPresenterEventId)
		this.setVariableValues({
			presenter_event_count: this.presenterEvents.length,
			presenter_selected_event: selected?.title ?? '',
		})
		this.updateDefinitions()
		this.log('debug', `Loaded ${this.presenterEvents.length} presenter events`)
	}

	private updateDefinitions(): void {
		this.setVariableDefinitions(GetVariableDefinitions(this))
		this.setVariableValues(GetDefaultVariableValues(this))
		this.setActionDefinitions(GetActions(this))
		this.setFeedbackDefinitions(GetFeedbacks(this))
		this.setPresetDefinitions(GetPresets(this))
	}

	private startPolling(): void {
		if (this.pollTimer) clearInterval(this.pollTimer)
		this.pollTimer = setInterval(async () => {
			if (this.isConnected) {
				await this.refreshCommands()
			}
		}, this.config.pollInterval)
	}

	private stopPolling(): void {
		if (this.pollTimer) {
			clearInterval(this.pollTimer)
			this.pollTimer = null
		}
	}

	private startPresentationPolling(): void {
		if (this.presentationPollTimer) clearInterval(this.presentationPollTimer)
		this.presentationPollTimer = setInterval(() => {
			if (this.isConnected) this.api.sendWsCommand('presentation.status')
		}, 2000)
	}

	private stopPresentationPolling(): void {
		if (this.presentationPollTimer) {
			clearInterval(this.presentationPollTimer)
			this.presentationPollTimer = null
		}
	}
}

runEntrypoint(ModuleInstance, [])
