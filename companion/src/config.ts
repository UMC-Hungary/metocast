import type { SomeCompanionConfigField } from '@companion-module/base'
import type { ModuleConfig } from './types.js'

export function GetConfigFields(): SomeCompanionConfigField[] {
	return [
		{
			type: 'static-text',
			id: 'info',
			width: 12,
			label: 'Information',
			value: 'Connect to Metocast to control Broadlink IR/RF devices and Keynote presentations.',
		},
		{
			type: 'textinput',
			id: 'host',
			label: 'Host',
			width: 8,
			default: '127.0.0.1',
			tooltip: 'IP address or hostname of the computer running Metocast',
		},
		{
			type: 'number',
			id: 'port',
			label: 'Port',
			width: 4,
			default: 3737,
			min: 1,
			max: 65535,
			tooltip: 'Metocast WebSocket port (default 3737)',
		},
		{
			type: 'textinput',
			id: 'authToken',
			label: 'Auth Token',
			width: 12,
			default: '',
			tooltip: 'Authentication token from Metocast settings',
		},
		{
			type: 'number',
			id: 'pollInterval',
			label: 'Command Poll Interval (ms)',
			width: 6,
			default: 30000,
			min: 5000,
			max: 300000,
			tooltip: 'How often to refresh the command list from the server',
		},
	]
}

export function GetDefaultConfig(): ModuleConfig {
	return {
		host: '127.0.0.1',
		port: 3737,
		authToken: '',
		pollInterval: 30000,
	}
}
