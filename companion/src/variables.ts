import type { CompanionVariableDefinition } from '@companion-module/base'
import type { ModuleInstance } from './main.js'

export function GetVariableDefinitions(_instance: ModuleInstance): CompanionVariableDefinition[] {
	return [
		{ variableId: 'connection_status', name: 'Connection Status' },
		{ variableId: 'last_command', name: 'Last Executed Command' },
		{ variableId: 'command_count', name: 'Total Available Commands' },
		{ variableId: 'server_host', name: 'Server Host' },
		{ variableId: 'server_port', name: 'Server Port' },
		{ variableId: 'ppt_filter', name: 'PPT Filter' },
		{ variableId: 'ppt_match_count', name: 'PPT Matches' },
		{ variableId: 'ppt_folder_name', name: 'PPT Folder Name' },
		{ variableId: 'ppt_slot_1_name', name: 'PPT Slot 1' },
		{ variableId: 'ppt_slot_2_name', name: 'PPT Slot 2' },
		{ variableId: 'ppt_slot_3_name', name: 'PPT Slot 3' },
		{ variableId: 'ppt_slot_4_name', name: 'PPT Slot 4' },
		{ variableId: 'ppt_slot_5_name', name: 'PPT Slot 5' },
		{ variableId: 'ppt_last_opened', name: 'PPT Last Opened File' },
		{ variableId: 'ppt_current_slide', name: 'Presentation Current Slide' },
		{ variableId: 'ppt_total_slides', name: 'Presentation Total Slides' },
		{ variableId: 'ppt_slideshow_active', name: 'Presentation Slideshow Active' },
		{ variableId: 'ppt_app', name: 'Presentation App Name' },
		{ variableId: 'ppt_backend', name: 'Presentation Backend' },
		{ variableId: 'ppt_blanked', name: 'Presentation Blanked' },
		{ variableId: 'ppt_document', name: 'Current Presentation File Name' },
		{ variableId: 'presenter_event_count', name: 'Presenter Event Count' },
		{ variableId: 'presenter_selected_event', name: 'Backend Selected Presenter Event' },
	]
}

export function GetDefaultVariableValues(instance: ModuleInstance): Record<string, string | number | undefined> {
	const pptState = instance.pptSelector.getState()

	return {
		connection_status: instance.isConnected ? 'Connected' : 'Disconnected',
		last_command: '',
		command_count: instance.commands.length,
		server_host: instance.config.host,
		server_port: instance.config.port,
		ppt_filter: pptState.currentFilter,
		ppt_match_count: pptState.matchingFiles.length.toString(),
		ppt_folder_name: instance.pptSelector.getSelectedFolderName(),
		ppt_slot_1_name: instance.pptSelector.getSlotDisplayName(0),
		ppt_slot_2_name: instance.pptSelector.getSlotDisplayName(1),
		ppt_slot_3_name: instance.pptSelector.getSlotDisplayName(2),
		ppt_slot_4_name: instance.pptSelector.getSlotDisplayName(3),
		ppt_slot_5_name: instance.pptSelector.getSlotDisplayName(4),
		ppt_last_opened: pptState.lastOpenedFile || '',
		ppt_current_slide: instance.presentationStatus?.currentSlide?.toString() ?? '-',
		ppt_total_slides: instance.presentationStatus?.totalSlides?.toString() ?? '-',
		ppt_slideshow_active: instance.presentationStatus?.slideshowActive ? 'ON' : 'OFF',
		ppt_app: instance.useWebPresenter ? 'Web Presenter' : (instance.presentationStatus?.app ?? 'None'),
		ppt_backend: instance.useWebPresenter ? 'Web Presenter' : 'Keynote',
		ppt_blanked: instance.presentationStatus?.blanked ? 'YES' : 'NO',
		ppt_document: instance.presentationStatus?.currentSlideTitle ?? '',
		presenter_event_count: instance.presenterEvents.length,
		presenter_selected_event:
			instance.presenterEvents.find((event) => event.id === instance.selectedPresenterEventId)?.title ?? '',
	}
}
