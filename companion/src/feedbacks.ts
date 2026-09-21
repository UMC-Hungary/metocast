import { combineRgb, type CompanionFeedbackDefinitions } from '@companion-module/base'
import type { ModuleInstance } from './main.js'

export function GetFeedbacks(instance: ModuleInstance): CompanionFeedbackDefinitions {
	return {
		connection_status: {
			type: 'boolean',
			name: 'Connection Status',
			description: 'Shows whether connected to the Metocast server',
			defaultStyle: {
				bgcolor: combineRgb(0, 200, 0),
				color: combineRgb(255, 255, 255),
			},
			options: [],
			callback: () => {
				return instance.isConnected
			},
		},

		command_available: {
			type: 'boolean',
			name: 'Command Available',
			description: 'Shows if a specific command is available',
			defaultStyle: {
				bgcolor: combineRgb(0, 100, 200),
				color: combineRgb(255, 255, 255),
			},
			options: [
				{
					type: 'dropdown',
					id: 'slug',
					label: 'Command',
					default: '',
					choices: instance.commands.map((cmd) => ({
						id: cmd.slug,
						label: cmd.name,
					})),
					allowCustom: true,
				},
			],
			callback: (feedback) => {
				const slug = feedback.options['slug'] as string
				return instance.commands.some((c) => c.slug === slug)
			},
		},

		// PPT Selector Feedbacks
		ppt_slot_has_file: {
			type: 'boolean',
			name: 'PPT: Slot Has File',
			description: 'Shows if a PPT slot has a file available',
			defaultStyle: {
				bgcolor: combineRgb(0, 150, 0),
				color: combineRgb(255, 255, 255),
			},
			options: [
				{
					type: 'dropdown',
					id: 'slot',
					label: 'Slot',
					default: '1',
					choices: [
						{ id: '1', label: 'Slot 1' },
						{ id: '2', label: 'Slot 2' },
						{ id: '3', label: 'Slot 3' },
						{ id: '4', label: 'Slot 4' },
						{ id: '5', label: 'Slot 5' },
					],
				},
			],
			callback: (feedback) => {
				const slot = parseInt(feedback.options['slot'] as string, 10) - 1
				return instance.pptSelector.getFileAtSlot(slot) !== null
			},
		},

		ppt_filter_active: {
			type: 'boolean',
			name: 'PPT: Filter Active',
			description: 'Shows if a PPT filter is currently applied',
			defaultStyle: {
				bgcolor: combineRgb(200, 100, 0),
				color: combineRgb(255, 255, 255),
			},
			options: [],
			callback: () => {
				return instance.pptSelector.getState().currentFilter.length > 0
			},
		},

		// Presentation Control Feedbacks
		slideshow_active: {
			type: 'boolean',
			name: 'Presentation: Slideshow Active',
			description: 'Shows if a slideshow is currently running',
			defaultStyle: {
				bgcolor: combineRgb(0, 150, 0),
				color: combineRgb(255, 255, 255),
			},
			options: [],
			callback: () => {
				return instance.presentationStatus?.slideshowActive ?? false
			},
		},

		presentation_blanked: {
			type: 'boolean',
			name: 'Presentation: Screen Blanked',
			description: 'Shows if the presentation screen is blanked (black)',
			defaultStyle: {
				bgcolor: combineRgb(0, 0, 0),
				color: combineRgb(255, 255, 255),
			},
			options: [],
			callback: () => {
				return instance.presentationStatus?.blanked ?? false
			},
		},
	}
}
