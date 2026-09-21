import type { PptFolder, PptFile } from './types.js'
import type { MetocastApi } from './api.js'

export interface PptSelectorState {
	currentFilter: string
	matchingFiles: PptFile[]
	selectedFolderId: string | null
	folders: PptFolder[]
	lastOpenedFile: string | null
}

export class PptSelector {
	private api: MetocastApi
	private state: PptSelectorState
	private onStateChange?: (state: PptSelectorState) => void

	constructor(api: MetocastApi) {
		this.api = api
		this.state = {
			currentFilter: '',
			matchingFiles: [],
			selectedFolderId: null,
			folders: [],
			lastOpenedFile: null,
		}
	}

	getState(): PptSelectorState {
		return { ...this.state }
	}

	getCurrentFilter(): string {
		return this.state.currentFilter
	}

	setOnStateChange(callback: (state: PptSelectorState) => void): void {
		this.onStateChange = callback
	}

	private notifyChange(): void {
		this.onStateChange?.(this.getState())
	}

	async refreshFolders(): Promise<void> {
		const folders = await this.api.getPptFolders()
		this.state.folders = folders

		if (!this.state.selectedFolderId && folders.length > 0) {
			this.state.selectedFolderId = folders[0].id
			await this.refreshFiles()
		}

		this.notifyChange()
	}

	updateFolders(folders: PptFolder[]): void {
		this.state.folders = folders

		if (this.state.selectedFolderId && !folders.find((f) => f.id === this.state.selectedFolderId)) {
			this.state.selectedFolderId = folders.length > 0 ? folders[0].id : null
			this.state.matchingFiles = []
			this.state.currentFilter = ''
		}

		this.notifyChange()
	}

	async refreshFiles(): Promise<void> {
		const response = await this.api.getPptFiles(this.state.currentFilter || undefined)
		this.state.matchingFiles = response.files
		this.notifyChange()
	}

	async selectFolder(folderId: string): Promise<void> {
		const folder = this.state.folders.find((f) => f.id === folderId)
		if (!folder) return

		this.state.selectedFolderId = folderId
		this.state.currentFilter = ''
		await this.refreshFiles()
	}

	async appendDigit(digit: number): Promise<void> {
		if (digit < 0 || digit > 9) return
		this.state.currentFilter += digit.toString()
		await this.refreshFiles()
	}

	async backspace(): Promise<void> {
		if (this.state.currentFilter.length === 0) return
		this.state.currentFilter = this.state.currentFilter.slice(0, -1)
		await this.refreshFiles()
	}

	async clearFilter(): Promise<void> {
		if (this.state.currentFilter === '') return
		this.state.currentFilter = ''
		await this.refreshFiles()
	}

	getFileAtSlot(slot: number): PptFile | null {
		if (slot < 0 || slot >= this.state.matchingFiles.length) return null
		return this.state.matchingFiles[slot]
	}

	async openFileAtSlot(slot: number): Promise<{ success: boolean; error?: string }> {
		const file = this.getFileAtSlot(slot)
		if (!file) return { success: false, error: 'No file at this slot' }

		const result = await this.api.openPptFile(file.path)
		if (result.success) {
			this.state.lastOpenedFile = file.name
		}

		this.state.currentFilter = ''
		await this.refreshFiles()

		return result
	}

	getSlotDisplayName(slot: number, maxLength = 15): string {
		const file = this.getFileAtSlot(slot)
		if (!file) return ''

		const nameWithoutExt = file.name.replace(/\.(pptx?|odp)$/i, '')
		if (nameWithoutExt.length <= maxLength) return nameWithoutExt
		return nameWithoutExt.slice(0, maxLength - 1) + '…'
	}

	getSelectedFolderName(): string {
		if (!this.state.selectedFolderId) return ''
		const folder = this.state.folders.find((f) => f.id === this.state.selectedFolderId)
		return folder?.name || ''
	}

	getFolderChoices(): Array<{ id: string; label: string }> {
		return this.state.folders.map((f) => ({ id: f.id, label: f.name }))
	}
}
