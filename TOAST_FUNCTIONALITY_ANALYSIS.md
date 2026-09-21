# Toast Notification Functionalities Analysis

This document outlines all toast notification use cases in the metocast system, organized by functionality and severity level. Use this as a prompt for design AI tools to improve notification patterns, messaging, icons, animations, and layouts.

## Toast Categories Overview

### 1. **System Notifications (Informational)**

#### 1.1 Application Updates

- **Type**: Info
- **Trigger**: App startup (UpdateChecker.svelte)
- **Message Pattern**: "New version available: {version}"
- **Action**: Interactive - includes "Download" action button
- **Context**: User is informed that an update is available with option to download immediately
- **Frequency**: Infrequent (on app startup)

#### 1.2 Event Management

- **Type**: Success / Info
- **Trigger**: Server event change notifications (ws/client.ts)
- **Messages**:
  - Success: "New event created: {title}"
  - Info: "Event updated: {title}"
- **Context**: User initiated or server-triggered events are communicated
- **Data included**: Event name/title
- **Frequency**: Occasional (per event change)

#### 1.3 Recording Detection

- **Type**: Success / Info
- **Trigger**: New recording detected (ws/client.ts)
- **Messages**:
  - Success: "Recording added to {eventTitle}"
  - Info: "New untracked recording detected"
- **Context**: System detects new recordings; may or may not be associated with an event
- **Data included**: Event title (if available)
- **Frequency**: Occasional (when recordings are added)

### 2. **Success Notifications**

#### 2.1 Upload Completion

- **Type**: Success
- **Trigger**: Recording successfully uploaded (ws/client.ts)
- **Message Pattern**: "Recording uploaded to {platform}"
- **Data included**: Platform name (YouTube, Facebook, etc.)
- **Context**: Long-running upload operation completed successfully
- **Frequency**: Occasional (per upload)
- **Related State**: Upload progress is tracked in real-time before completion

#### 2.2 Settings Saved

- **Type**: Success
- **Trigger**: Caption settings saved (obs-caption/+page.svelte)
- **Message**: "Caption settings saved"
- **Context**: Configuration change has been persisted
- **Frequency**: Per user action

#### 2.3 Connector Settings Saved

- **Type**: Success / Error
- **Trigger**: Saving any connector's settings (ConnectorSettingsBlock.svelte)
- **Messages**:
  - Success: "{connector} settings saved"
  - Error: "Could not save {connector} settings" with the failure as the description
- **Data included**: Connector display name
- **Context**: Confirms the config reached the core, which is otherwise silent — the form has no other success indication
- **Frequency**: Per user action

#### 2.4 Clipboard Copy

- **Type**: Success
- **Trigger**: Caption URL copied to clipboard (obs-caption/+page.svelte)
- **Message**: "Caption URL copied to clipboard"
- **Context**: User action to copy URL completed successfully
- **Duration**: Short display (auto-close after 2 seconds)
- **UI Change**: Button text changes from "Copy URL" to "Copied!"
- **Frequency**: Per user action

### 3. **Error Notifications**

#### 3.1 WebSocket Communication Errors

- **Type**: Error
- **Trigger**: WebSocket error message received (ws/client.ts)
- **Message Pattern**: Raw error message from server
- **Context**: Network or server communication failure
- **Frequency**: On error events

#### 3.2 Upload Failures

- **Type**: Error
- **Trigger**: Recording upload fails (ws/client.ts)
- **Message Pattern**: "Upload to {platform} failed: {error}"
- **Data included**: Platform name, error details
- **Context**: Long-running upload operation failed
- **Frequency**: On upload failure

#### 3.3 Settings Load/Save Failures

- **Type**: Error
- **Trigger**: Caption settings operations fail (obs-caption/+page.svelte)
- **Messages**:
  - "Failed to load caption settings"
  - "Failed to save caption settings"
- **Context**: Data persistence or retrieval failed
- **Frequency**: On operation failure

### 4. **Warning Notifications**

#### 4.1 General Warnings

- **Type**: Warning
- **Trigger**: Notification message with 'warn' level (ws/client.ts)
- **Message Pattern**: Raw warning message from server
- **Context**: Non-critical issue or advisory message
- **Frequency**: On warning events

### 5. **Generic Message Notifications**

#### 5.1 Server Notifications

- **Type**: Auto-selected based on level (info/warn/error)
- **Trigger**: Generic notification message from server (ws/client.ts)
- **Message Pattern**: Raw message with mapped severity level
- **Levels**: 'warn' → Warning, 'error' → Error, others → Info
- **Context**: Flexible server-driven messaging
- **Frequency**: Per server notification

---

## Toast Design Recommendations

### Message Structure

- **Pattern 1**: Simple confirmation → "Action completed"
- **Pattern 2**: Entity-focused → "{Action} {Entity}: {Name}"
- **Pattern 3**: System-focused → "{System} {Action}: {Details}"

### Duration Patterns

- **Persistent with action**: Update availability (user must take action)
- **Auto-dismiss (3-5s)**: Success notifications, simple confirmations
- **Auto-dismiss (2s)**: Quick feedback (copy, toggle)
- **Sticky/Manual dismiss**: Errors, critical warnings

### Icon Recommendations

- Success (green): ✓ Check mark, upload complete, save success, copy success
- Error (red): ✗ X mark, failed operations, connection issues
- Warning (yellow): ⚠ Alert triangle for warnings
- Info (blue): ℹ Info circle for updates, status changes
- Action (blue): → Arrow or action icon with CTA

### Color Coding

- Uses `svelte-sonner` richColors feature
- Green for success
- Red for errors
- Yellow for warnings
- Blue for info/updates

### Interactive Elements

- **Action Buttons**: Update download link (UpdateChecker)
- **Copy Feedback**: URL copy with state change

### Data Context

- **Dynamic content**: Event titles, platform names, error messages
- **Temporal data**: Upload progress (separate from toast)
- **Related UI feedback**: Button state changes (e.g., "Copy URL" → "Copied!")

---

## Implementation Notes

### Current Library

- **Library**: svelte-sonner
- **Configuration**:
  - Position: top-right
  - Rich colors enabled
  - Close button visible

### State Management

- Upload progress tracked separately in reactive store
- Notification display independent of state updates
- Server-driven notifications via WebSocket messages

### Error Handling

- Silent failures on app startup (updater check)
- Explicit error display on user actions
- Raw error messages from server passed through

### Localization

- Some messages use i18n (UpdateChecker)
- Most messages are dynamic/English strings from server or code

---

## Design Prompt Template

**For UI/UX Designer or Design AI Tool:**

"Design a comprehensive toast notification system for a desktop streaming control application with the following requirements:

**Notification Types to Support:**

1. System updates with downloadable action
2. Event management (create/update)
3. Recording detection (tracked/untracked)
4. Long-running operations (upload progress → completion/failure)
5. Settings changes (load/save success/failure)
6. Quick feedback (clipboard copy)
7. Generic messages (info/warn/error from server)

**Requirements:**

- Support 4 severity levels: info, success, warning, error
- Auto-dismiss for most notifications (2-5 second duration)
- Persistent dismissible format for errors and updates
- Interactive CTA buttons for some notifications (e.g., download link)
- Dynamic content insertion (names, platforms, error details)
- Rich color coding using semantic colors (green/red/yellow/blue)
- Top-right corner positioning
- Visual feedback state changes (e.g., button text updates)
- Accessibility considerations

**Optional Enhancements:**

- Progress indicators for long-running operations
- Sound notifications for critical events
- Toast grouping for similar notifications
- Dismissal gestures (swipe to dismiss)
- Animation variations by type/importance"
