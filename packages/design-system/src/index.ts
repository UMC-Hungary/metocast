/**
 * The package's only public entry point. Consumers import from here; nothing reaches into
 * `src/primitives` or `src/patterns` directly, so a component's internals stay internal.
 */

// Primitives
export { default as Badge } from './primitives/Badge.svelte';
export { default as Button } from './primitives/Button.svelte';
export { default as Checkbox } from './primitives/Checkbox.svelte';
export { default as DateBlock } from './primitives/DateBlock.svelte';
export { default as Dot } from './primitives/Dot.svelte';
export { default as Field } from './primitives/Field.svelte';
export { default as FlameMark } from './primitives/FlameMark.svelte';
export { default as FormField } from './primitives/FormField.svelte';
export { default as Glyph } from './primitives/Glyph.svelte';
export { default as Icon } from './primitives/Icon.svelte';
export { default as IconButton } from './primitives/IconButton.svelte';
export { default as List } from './primitives/List.svelte';
export { default as Lockup } from './primitives/Lockup.svelte';
export { default as OverviewCell } from './primitives/OverviewCell.svelte';
export { default as PageHeader } from './primitives/PageHeader.svelte';
export { default as ProgressBar } from './primitives/ProgressBar.svelte';
export { default as RadioGroup } from './primitives/RadioGroup.svelte';
export { default as Row } from './primitives/Row.svelte';
export { default as SectionLabel } from './primitives/SectionLabel.svelte';
export { default as Segmented } from './primitives/Segmented.svelte';
export { default as Select } from './primitives/Select.svelte';
export { default as Skeleton } from './primitives/Skeleton.svelte';
export { default as Spinner } from './primitives/Spinner.svelte';
export { default as Stat } from './primitives/Stat.svelte';
export { default as StatusDot } from './primitives/StatusDot.svelte';
export { default as Tabs } from './primitives/Tabs.svelte';
export { default as TextArea } from './primitives/TextArea.svelte';
export { default as TextField } from './primitives/TextField.svelte';
export { default as TextIcon } from './primitives/TextIcon.svelte';
export { default as Toggle } from './primitives/Toggle.svelte';
export { default as Tooltip } from './primitives/Tooltip.svelte';

// Patterns
export { default as DeviceList } from './patterns/DeviceList.svelte';
export { default as Dialog } from './patterns/Dialog.svelte';
export { default as DiscoveryPanel } from './patterns/DiscoveryPanel.svelte';
export { default as EmptyState } from './patterns/EmptyState.svelte';
export { default as ErrorState } from './patterns/ErrorState.svelte';
export { default as FormSection } from './patterns/FormSection.svelte';
export { default as LabelledInput } from './patterns/LabelledInput.svelte';
export { default as NativeDateInput } from './patterns/NativeDateInput.svelte';
export { default as NavigationBar } from './patterns/NavigationBar.svelte';
export { default as NotificationBell } from './patterns/NotificationBell.svelte';
export { default as NotificationCentre } from './patterns/NotificationCentre.svelte';
export { default as PresenterPanel } from './patterns/PresenterPanel.svelte';
export { default as ReferenceInput } from './patterns/ReferenceInput.svelte';
export { default as Sheet } from './patterns/Sheet.svelte';
export { default as SlideQueue } from './patterns/SlideQueue.svelte';
export { default as SlideSearch } from './patterns/SlideSearch.svelte';
export { default as StickyActionBar } from './patterns/StickyActionBar.svelte';
export { default as Table } from './patterns/Table.svelte';
export { default as Toast } from './patterns/Toast.svelte';
export { default as ToastOverlay } from './patterns/ToastOverlay.svelte';
export { default as ToggleRow } from './patterns/ToggleRow.svelte';
export { default as TransportDock } from './patterns/TransportDock.svelte';

// Accessibility behaviours, carried over unchanged from the previous design system.
export { default as LiveRegion } from './a11y/LiveRegion.svelte';
export { dismissable } from './a11y/dismissable.js';
export { announce, assertiveMessage, politeMessage } from './a11y/live-region.js';
export { rovingTabindex } from './a11y/roving-tabindex.js';

export type { IconName } from './primitives/Icon.svelte';
export type { RadioOption } from './primitives/RadioGroup.svelte';
export type { SegmentedOption } from './primitives/Segmented.svelte';
export type { SelectOption } from './primitives/Select.svelte';
export type { Status } from './primitives/StatusDot.svelte';
export type { Tab } from './primitives/Tabs.svelte';
export type { FormFieldContext } from './primitives/FormField.svelte';
export type { Device } from './patterns/DeviceList.svelte';
export type { NavItem } from './patterns/NavigationBar.svelte';
export type { PresenterClient } from './patterns/PresenterPanel.svelte';
export type { QueueSlot } from './patterns/SlideQueue.svelte';
export type { SlideResult } from './patterns/SlideSearch.svelte';
export type { Column } from './patterns/Table.svelte';
export type { ToastAction, ToastTone } from './patterns/Toast.svelte';
export type { TransportAction } from './patterns/TransportDock.svelte';
export type { ReferenceResult, Verse } from './patterns/ReferenceInput.svelte';
export type { DismissableOptions } from './a11y/dismissable.js';
export type { RovingOptions } from './a11y/roving-tabindex.js';
