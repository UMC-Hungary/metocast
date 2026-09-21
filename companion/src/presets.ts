import { combineRgb, type CompanionPresetDefinitions } from '@companion-module/base'
import type { ModuleInstance } from './main.js'
import { CATEGORY_COLORS } from './types.js'

export function GetPresets(instance: ModuleInstance): CompanionPresetDefinitions {
	const presets: CompanionPresetDefinitions = {}
	const variablePrefix = instance.label
	const stopOrUnloadName = instance.useWebPresenter ? 'Unload Web Presenter' : 'Stop Slideshow'
	const stopOrUnloadText = instance.useWebPresenter ? '⏹ Unload' : '⏹ Stop'

	// Generate a preset for each command
	for (const cmd of instance.commands) {
		const categoryColor = CATEGORY_COLORS[cmd.category] ?? CATEGORY_COLORS['other']

		const r = (categoryColor >> 16) & 0xff
		const g = (categoryColor >> 8) & 0xff
		const b = categoryColor & 0xff

		presets[`cmd_${cmd.slug}`] = {
			type: 'button',
			category: cmd.category.charAt(0).toUpperCase() + cmd.category.slice(1),
			name: cmd.name,
			style: {
				text: cmd.name,
				size: 'auto',
				color: combineRgb(255, 255, 255),
				bgcolor: combineRgb(r, g, b),
			},
			steps: [
				{
					down: [
						{
							actionId: 'execute_command',
							options: {
								slug: cmd.slug,
							},
						},
					],
					up: [],
				},
			],
			feedbacks: [
				{
					feedbackId: 'connection_status',
					options: {},
					style: {
						bgcolor: combineRgb(100, 100, 100),
					},
					isInverted: true,
				},
			],
		}
	}

	// Status preset
	presets['status'] = {
		type: 'button',
		category: 'Status',
		name: 'Connection Status',
		style: {
			text: 'Meto\\nStream',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(100, 0, 0),
		},
		steps: [
			{
				down: [
					{
						actionId: 'refresh_commands',
						options: {},
					},
				],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: {
					bgcolor: combineRgb(0, 100, 0),
				},
			},
		],
	}

	// PPT Selector Presets - Digit Buttons
	for (let digit = 0; digit <= 9; digit++) {
		presets[`ppt_digit_${digit}`] = {
			type: 'button',
			category: 'PPT Selector',
			name: `PPT: Digit ${digit}`,
			style: {
				text: digit.toString(),
				size: '44',
				color: combineRgb(255, 255, 255),
				bgcolor: combineRgb(59, 130, 246),
			},
			steps: [
				{
					down: [
						{
							actionId: 'ppt_digit',
							options: {
								digit: digit.toString(),
							},
						},
					],
					up: [],
				},
			],
			feedbacks: [
				{
					feedbackId: 'connection_status',
					options: {},
					style: {
						bgcolor: combineRgb(100, 100, 100),
					},
					isInverted: true,
				},
			],
		}
	}

	presets['ppt_backspace'] = {
		type: 'button',
		category: 'PPT Selector',
		name: 'PPT: Backspace',
		style: {
			text: '⌫',
			size: '44',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(239, 68, 68),
		},
		steps: [
			{
				down: [
					{
						actionId: 'ppt_backspace',
						options: {},
					},
				],
				up: [],
			},
		],
		feedbacks: [],
	}

	presets['ppt_clear'] = {
		type: 'button',
		category: 'PPT Selector',
		name: 'PPT: Clear',
		style: {
			text: 'CLR',
			size: '18',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(245, 158, 11),
		},
		steps: [
			{
				down: [
					{
						actionId: 'ppt_clear',
						options: {},
					},
				],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'ppt_filter_active',
				options: {},
				style: {
					bgcolor: combineRgb(220, 120, 0),
				},
			},
		],
	}

	presets['ppt_refresh'] = {
		type: 'button',
		category: 'PPT Selector',
		name: 'PPT: Refresh',
		style: {
			text: '↻',
			size: '44',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(107, 114, 128),
		},
		steps: [
			{
				down: [
					{
						actionId: 'ppt_refresh',
						options: {},
					},
				],
				up: [],
			},
		],
		feedbacks: [],
	}

	for (let slot = 1; slot <= 5; slot++) {
		presets[`ppt_slot_${slot}`] = {
			type: 'button',
			category: 'PPT Selector',
			name: `PPT: Slot ${slot}`,
			style: {
				text: `$(${variablePrefix}:ppt_slot_${slot}_name)`,
				size: '14',
				color: combineRgb(255, 255, 255),
				bgcolor: combineRgb(100, 100, 100),
			},
			steps: [
				{
					down: [
						{
							actionId: 'ppt_select_slot',
							options: {
								slot: slot.toString(),
							},
						},
					],
					up: [],
				},
			],
			feedbacks: [
				{
					feedbackId: 'ppt_slot_has_file',
					options: {
						slot: slot.toString(),
					},
					style: {
						bgcolor: combineRgb(0, 150, 0),
					},
				},
				{
					feedbackId: 'connection_status',
					options: {},
					style: {
						bgcolor: combineRgb(80, 80, 80),
					},
					isInverted: true,
				},
			],
		}
	}

	presets['ppt_filter_status'] = {
		type: 'button',
		category: 'PPT Selector',
		name: 'PPT: Filter Status',
		style: {
			text: `Filter:\\n$(${variablePrefix}:ppt_filter)\\n$(${variablePrefix}:ppt_match_count) files`,
			size: '14',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(50, 50, 50),
		},
		steps: [{ down: [], up: [] }],
		feedbacks: [
			{
				feedbackId: 'ppt_filter_active',
				options: {},
				style: {
					bgcolor: combineRgb(100, 50, 0),
				},
			},
		],
	}

	presets['ppt_folder_display'] = {
		type: 'button',
		category: 'PPT Selector',
		name: 'PPT: Current Folder',
		style: {
			text: `📁\\n$(${variablePrefix}:ppt_folder_name)`,
			size: '14',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(70, 70, 70),
		},
		steps: [{ down: [], up: [] }],
		feedbacks: [],
	}

	// Presentation Control Presets
	presets['presentation_open'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Open Presentation',
		style: {
			text: '📂 Open',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(100, 65, 165),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_open', options: { filePath: '' } }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_on_off'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Presentation ON/OFF',
		style: {
			text: 'Presentation\\nOFF',
			size: '14',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(100, 0, 0),
		},
		steps: [{ down: [], up: [] }],
		feedbacks: [
			{
				feedbackId: 'slideshow_active',
				options: {},
				style: { text: 'Presentation\\nON', bgcolor: combineRgb(0, 150, 0) },
			},
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(80, 80, 80) },
				isInverted: true,
			},
		],
	}

	presets['presentation_start'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Start Slideshow',
		style: {
			text: '▶ Play',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(0, 150, 0),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_start', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_stop'] = {
		type: 'button',
		category: 'Presentation Control',
		name: stopOrUnloadName,
		style: {
			text: stopOrUnloadText,
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(153, 0, 0),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_stop', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'slideshow_active',
				options: {},
				style: { bgcolor: combineRgb(200, 0, 0) },
			},
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_close'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Close All Presentations',
		style: {
			text: '✕ Close\\nAll',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(130, 0, 0),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_close', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_close_latest'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Close Latest Presentation',
		style: {
			text: '✕ Close',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(153, 0, 0),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_close_latest', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_prev'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Previous Slide',
		style: {
			text: '◀ Prev',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(59, 130, 246),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_previous', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_next'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Next Slide',
		style: {
			text: '▶ Next',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(59, 130, 246),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_next', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_first'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'First Slide',
		style: {
			text: '⏮ First',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(59, 130, 246),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_first', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_last'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Last Slide',
		style: {
			text: '⏭ Last',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(59, 130, 246),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_last', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_blank'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Toggle Blank Screen',
		style: {
			text: '■ Blank',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(50, 50, 50),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_blank_toggle', options: {} }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'presentation_blanked',
				options: {},
				style: { bgcolor: combineRgb(0, 0, 0) },
			},
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_goto'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Go to Slide',
		style: {
			text: 'Go To\\nSlide',
			size: 'auto',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(100, 65, 165),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_goto', options: { slideNumber: 1 } }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_status'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Slide Status',
		style: {
			text: `$(${variablePrefix}:ppt_document)\\n$(${variablePrefix}:ppt_current_slide)/$(${variablePrefix}:ppt_total_slides)`,
			size: '14',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(50, 50, 50),
		},
		steps: [{ down: [], up: [] }],
		feedbacks: [
			{
				feedbackId: 'slideshow_active',
				options: {},
				style: { bgcolor: combineRgb(0, 100, 0) },
			},
		],
	}

	presets['presentation_bible_textus'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Show Textus',
		style: {
			text: 'Textus',
			size: '18',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(30, 120, 95),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_bible_reference', options: { referenceType: 'textus', eventId: '' } }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['presentation_bible_leckio'] = {
		type: 'button',
		category: 'Presentation Control',
		name: 'Show Lekcio',
		style: {
			text: 'Lekcio',
			size: '18',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(120, 85, 30),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_bible_reference', options: { referenceType: 'leckio', eventId: '' } }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	presets['ppt_open_target'] = {
		type: 'button',
		category: 'PPT Selector',
		name: 'PPT: Open Target File',
		style: {
			text: '📂 Open\\nTarget',
			size: '14',
			color: combineRgb(255, 255, 255),
			bgcolor: combineRgb(100, 65, 165),
		},
		steps: [
			{
				down: [{ actionId: 'presentation_open', options: { filePath: '' } }],
				up: [],
			},
		],
		feedbacks: [
			{
				feedbackId: 'connection_status',
				options: {},
				style: { bgcolor: combineRgb(100, 100, 100) },
				isInverted: true,
			},
		],
	}

	return presets
}
