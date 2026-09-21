# System Connector Notifications Design Prompt

This document outlines the complete system notification architecture for hardware/platform connector status and error handling. Use this as a specification for designing notification systems, status dashboards, and error recovery workflows.

---

## System Overview

The application manages **8 interconnected connectors** with real-time status monitoring, error tracking, and recovery workflows:

```
STREAMING PLATFORMS     HARDWARE DEVICES        MESSAGING         TOOLS
├─ YouTube Live        ├─ OBS Studio           ├─ Discord         ├─ Broadlink
├─ Facebook Live       ├─ vMix                 │  Webhooks        │  (RF/IR Control)
└─ [Future]            ├─ ATEM Switcher        └─ [Future]        └─ OBS Device
                       └─ Broadlink (RF/IR)       Listeners
```

---

## Connector Status System

### 4-State Status Model

Every connector tracks one of these states:

| State            | Color  | Icon | Meaning                                            | User Action                        |
| ---------------- | ------ | ---- | -------------------------------------------------- | ---------------------------------- |
| **connected**    | Green  | ●    | Fully operational, ready to stream/control         | None required                      |
| **connecting**   | Yellow | ◐    | Attempting to establish connection                 | Wait or troubleshoot               |
| **disconnected** | Grey   | ○    | Not connected (may be disabled)                    | Enable or configure                |
| **error**        | Red    | ✕    | Connected but non-functional, or connection failed | See error details, recheck, or fix |

### Status Transitions

```
User enables → connecting ──→ connected
connector                          ↓
                              [operations]
                                  ↓
                    ┌─────────────┴─────────────┐
                    ↓                           ↓
            (status updates           (error detected)
             periodically)                    ↓
                    ↓                      error
            maintaining state              (sticky)
                    ↓
                disconnected (user disabled)
```

### Display Locations

1. **Navigation Bar** (`NavConnectors.svelte`)
   - Compact badge showing: `[dot] Name Status`
   - Visible when enabled
   - Real-time indicator

2. **Dashboard Widget** (`ConnectorDashboardWidget.svelte`)
   - Full status badge
   - Capability flags (streaming/recording/live)
   - Device-specific controls (Broadlink: device select + commands)

3. **Settings Page** (`ConnectorSettingsBlock.svelte`)
   - Configuration form
   - Enable/disable toggle
   - Validation status

---

## Error Management System

### Error Data Structure

```typescript
interface ConnectorError {
  id: string; // Unique error ID
  connectorId: string; // 'obs', 'youtube', 'discord', etc.
  connectorName: string; // Display name "OBS Studio"
  message: string; // User-facing error message
  infoMarkdown?: string; // Detailed troubleshooting steps
  timestamp: Date; // When error occurred
}
```

### Error Lifecycle

1. **Detection**: Server detects connector failure, pushes via WebSocket
2. **Display**: Error added to store, badge in nav turns red, error count increments
3. **User Views**: Opens `/errors` page, sees error card with:
   - Connector name + error message
   - "Recheck" button (re-runs status command)
   - "Fix" button (opens connector setup modal)
   - "Info" button (if infoMarkdown available)
4. **Resolution**: User fixes settings, reruns recheck or restarts service
5. **Cleanup**: Error cleared from store when connector reconnects

### Error Display (`/errors` page)

- **Error Count Badge**: Navigation shows count of active errors
- **Error Cards**: List of all connector errors with:
  - Error message (concise, user-facing)
  - Troubleshooting guide (markdown + HTML rendering)
  - Action buttons: Recheck, Fix, Show Info
  - Timestamp (implicit via error position)

---

## Connector-Specific Notifications

### 1. OBS Studio (`obs`)

**Category:** Software Device  
**Capabilities:** Streaming, Recording  
**Protocol:** WebSocket (port 4455 default)

#### Status Scenarios

| Scenario                | Message                                       | Recovery                                  |
| ----------------------- | --------------------------------------------- | ----------------------------------------- |
| Connected               | ✓ "OBS Studio Connected"                      | Ready to stream/record                    |
| Connecting              | ◐ "Connecting to OBS..."                      | Wait 5-10s, check host/port               |
| Disconnected            | ○ "OBS Studio Disconnected"                   | Enable OBS, check network                 |
| Error: Cannot connect   | ✕ "OBS connection failed: connection refused" | Check host, port, firewall                |
| Error: Wrong password   | ✕ "OBS auth failed: invalid password"         | Verify password in OBS WebSocket settings |
| Error: Version mismatch | ✕ "OBS version incompatible"                  | Update OBS to latest version              |

#### Troubleshooting Guide

- Enable WebSocket Server in OBS (Tools → WebSocket Server Settings)
- Note port number (default 4455)
- Set secure password
- Verify host/port in Settings match

#### Device Listeners (OBS Sources)

- Users can create "device listeners" for OBS sources
- Each listener monitored for availability
- **Error**: "Device unavailable: {device name}"
- **Recovery**: Add new listener, update reference

#### Related Notifications (via WS)

- `obs.state`: Stream started/stopped, recording started/stopped
- `obs.devices.available`: List of available OBS sources
- `obs.listeners.{list,create,update,delete}`: Listener lifecycle events

---

### 2. vMix (`vmix`)

**Category:** Software Device  
**Capabilities:** Streaming  
**Protocol:** HTTP/TCP (port 8088 default)

#### Status Scenarios

| Scenario              | Message                                      | Recovery                              |
| --------------------- | -------------------------------------------- | ------------------------------------- |
| Connected             | ✓ "vMix Connected"                           | Ready to control                      |
| Connecting            | ◐ "Connecting to vMix..."                    | Wait for vMix to start                |
| Disconnected          | ○ "vMix Disconnected"                        | Enable vMix, check IP                 |
| Error: Cannot connect | ✕ "vMix connection failed: host unreachable" | Check vMix is running, verify IP/port |
| Error: Network issue  | ✕ "vMix connection timeout"                  | Check network connectivity            |

#### Troubleshooting Guide

- Ensure vMix is running on the target machine
- Verify IP address and port (default 8088)
- Check firewall allows connections
- Confirm network connectivity between machines

---

### 3. ATEM Switcher (`atem`) - Blackmagic Design

**Category:** Hardware Device  
**Capabilities:** Streaming, Recording  
**Protocol:** UDP (port 9910 default)

#### Status Scenarios

| Scenario                   | Message                                 | Recovery                                |
| -------------------------- | --------------------------------------- | --------------------------------------- |
| Connected                  | ✓ "ATEM Connected"                      | Ready to control                        |
| Connecting                 | ◐ "Connecting to ATEM..."               | Power on device, check network          |
| Disconnected               | ○ "ATEM Disconnected"                   | Power on ATEM, check IP                 |
| Error: Cannot connect      | ✕ "ATEM connection failed: no response" | Check device is powered, on network     |
| Error: Network unreachable | ✕ "ATEM: network interface down"        | Check Ethernet cable, IP address        |
| Error: Firmware mismatch   | ✕ "ATEM firmware version incompatible"  | Update ATEM firmware via Blackmagic app |

#### Troubleshooting Guide

- Power on ATEM Switcher
- Verify Ethernet connection
- Check IP address in ATEM settings (button press on device)
- Confirm network routing between control machine and ATEM
- Update ATEM firmware if prompted

#### Capability Flags

- Streaming: Indicates device is actively switching
- Recording: Indicates device has recording input active

---

### 4. YouTube (`youtube`)

**Category:** Platform  
**Capabilities:** Live streaming (detection only)  
**Authentication:** OAuth 2.0 required

#### Status Scenarios

| Scenario                  | Message                            | Recovery                                      |
| ------------------------- | ---------------------------------- | --------------------------------------------- |
| Authenticated             | ✓ "YouTube Connected"              | Ready to broadcast                            |
| Connecting                | ◐ "Authenticating with YouTube..." | Waiting for OAuth                             |
| Disconnected              | ○ "YouTube Not Connected"          | Login required                                |
| Error: Auth expired       | ✕ "YouTube: session expired"       | Re-authenticate via Settings                  |
| Error: Quota exceeded     | ✕ "YouTube: API quota exceeded"    | Wait 24h or upgrade quota                     |
| Error: Account restricted | ✕ "YouTube: streaming disabled"    | Check account settings, enable live streaming |

#### Live Status Detection

- `youtubeLiveActive` flag: True when at least one live broadcast is active
- Polling via cron job (`cron.youtube_pull`)
- Display: "YouTube LIVE" badge with pulsing red dot in nav

#### Troubleshooting Guide

- Click "Login" to authorize with Google Account
- Ensure YouTube account has live streaming enabled
- Check quotas in Google Cloud Console
- Verify no geographic restrictions

---

### 5. Facebook (`facebook`)

**Category:** Platform  
**Capabilities:** Live streaming (detection only)  
**Authentication:** OAuth 2.0 (App credentials)

#### Status Scenarios

| Scenario                   | Message                             | Recovery                          |
| -------------------------- | ----------------------------------- | --------------------------------- |
| Authenticated              | ✓ "Facebook Connected"              | Ready to broadcast                |
| Connecting                 | ◐ "Authenticating with Facebook..." | Waiting for OAuth                 |
| Disconnected               | ○ "Facebook Not Connected"          | Configure app credentials         |
| Error: Invalid credentials | ✕ "Facebook: invalid app secret"    | Check App ID and Secret           |
| Error: Page not found      | ✕ "Facebook: page access denied"    | Verify page ID, check permissions |
| Error: Token revoked       | ✕ "Facebook: authentication failed" | Re-authenticate, check app status |

#### Troubleshooting Guide

- Create Facebook App in Developer Dashboard
- Obtain App ID and App Secret
- Get Page ID from Facebook page settings
- Ensure app has permission to broadcast

---

### 6. Discord (`discord`)

**Category:** Messaging Platform  
**Capabilities:** Announcements (webhook-based)  
**Protocol:** HTTP POST webhook

#### Status Scenarios

| Scenario               | Message                                | Recovery                                 |
| ---------------------- | -------------------------------------- | ---------------------------------------- |
| Connected              | ✓ "Discord Connected"                  | Ready to send messages                   |
| Connecting             | ◐ "Testing Discord webhook..."         | Wait for response                        |
| Disconnected           | ○ "Discord Not Configured"             | Add webhook URL                          |
| Error: Invalid URL     | ✕ "Discord: invalid webhook URL"       | Check URL format and syntax              |
| Error: Webhook deleted | ✕ "Discord: webhook not found (404)"   | Recreate webhook in Discord              |
| Error: Forbidden       | ✕ "Discord: insufficient permissions"  | Check channel permissions, webhook scope |
| Error: Rate limited    | ✕ "Discord: rate limited, retry in Xs" | Wait before retrying                     |

#### Troubleshooting Guide

- Go to Discord server → Server Settings → Integrations → Webhooks
- Create new webhook for target channel
- Copy webhook URL
- Paste into Settings, click "Test Connection"
- Verify no special characters in URL

#### Note

Discord integration status: "Not yet fully implemented" (per definition)

---

### 7. Broadlink (`broadlink`) - RF/IR Control

**Category:** Hardware Device (IR/RF Learning)  
**Capabilities:** Command sending (IR/RF)  
**Discovery:** Automatic via network broadcast

#### Status Scenarios

| Scenario                | Message                              | Recovery                            |
| ----------------------- | ------------------------------------ | ----------------------------------- |
| Connected               | ✓ "Broadlink Connected"              | Ready to send commands              |
| Connecting              | ◐ "Discovering Broadlink devices..." | Wait for discovery scan             |
| Disconnected            | ○ "No Broadlink Devices"             | Power on device, check network      |
| Error: No devices found | ✕ "Broadlink: no devices on network" | Ensure device powered, on same WiFi |
| Error: Learn timeout    | ✕ "Broadlink: learn mode timeout"    | Point remote at device, retrigger   |
| Error: Send failed      | ✕ "Broadlink: command send failed"   | Check device, verify learned code   |

#### Device Management

- **Discovery**: Automatic broadcast discovery
- **Device Selection**: Multiple devices supported, UI shows dropdown
- **Command Categories**: Projector, Screen, Lighting, Audio, HVAC, Other
- **Learn Mode**: Point IR remote at Broadlink device, learn code
- **Send**: Execute learned IR/RF command

#### Troubleshooting Guide

- Power on Broadlink device, connect to WiFi
- Device should appear in discovery automatically
- For IR/RF learning: press "Learn" button, point remote, press button
- If not discovered: check WiFi network, device reset

#### Related UI Elements

- Device selector dropdown
- Learned command buttons (grouped by category)
- Learn dialog for new commands
- Device list with edit/delete

---

### 8. OBS Badge (`obs-badge`) - Special Connector

**Category:** OBS Scene Indicator  
**Capabilities:** Scene monitoring  
**Use Case**: Display current OBS scene in UI/remote control

#### Status Scenarios

| Scenario               | Message                                 | Recovery                      |
| ---------------------- | --------------------------------------- | ----------------------------- |
| Connected              | ✓ "OBS Badge Connected"                 | Showing current scene         |
| Disconnected           | ○ "OBS Badge Disconnected"              | No scene display              |
| Error: Scene not found | ✕ "OBS Badge: scene '{name}' not found" | Update scene name in settings |

#### Configuration

- Requires OBS connection
- Scene name specified in settings
- Updates automatically when scene changes

---

## Error Recovery Workflow

### User Journey When Error Occurs

```
1. Error Detected
   └─ Server detects connector failure
   └─ Pushes error via WebSocket
   └─ Error added to store

2. Visual Indication
   ├─ Connector badge turns RED with "Error" label
   ├─ Error count badge appears in nav (shows count)
   └─ Toast notification may show (implementation-dependent)

3. User Discovers Issue
   └─ Sees red badge in navigation or dashboard
   └─ Clicks badge or error icon
   └─ Navigates to /errors page (or modal)

4. Error Details Display
   ├─ Connector name: "YouTube"
   ├─ Error message: "session expired"
   ├─ Buttons:
   │  ├─ "Recheck" — Re-test connection immediately
   │  ├─ "Fix" — Opens connector setup modal
   │  └─ "Info" — Shows troubleshooting guide
   └─ Troubleshooting guide (if available)
      └─ Markdown rendered as HTML with steps/links

5. Fix Attempt
   Option A: Recheck
   └─ User clicks "Recheck"
   └─ System invokes backend test command
   └─ If successful → error clears
   └─ If still broken → error persists, user can try Option B

   Option B: Fix Configuration
   └─ User clicks "Fix"
   └─ Opens ConnectorFixModal
   └─ Shows form for connector settings
   └─ User updates configuration (password, API key, URL, etc.)
   └─ User saves
   └─ Backend re-tests, updates status

   Option C: Manual Troubleshooting
   └─ User reads "Info" guide
   └─ Follows steps to fix (e.g., enable WebSocket in OBS)
   └─ Returns to app, clicks "Recheck"
   └─ System detects recovery

6. Resolution
   ├─ Error clears from store
   ├─ Badge returns to green "Connected"
   ├─ Error count decrements
   └─ User can resume operations
```

---

## Design Considerations for Notifications

### 1. Status Badge Design

**Requirements:**

- Quick visual recognition of connector health
- Minimal space in navigation bar
- Color-coded (green/yellow/grey/red)
- Shows: [colored dot] [name] [status label]
- Tooltip on hover (optional): Full connector name + last update time

**Animations:**

- Yellow pulse when connecting (5s interval)
- No animation when connected (solid green)
- Red steady when error
- Grey dim when disconnected

### 2. Error Card Design

**Requirements:**

- Clear connector name (bold)
- Concise error message
- Action buttons (Recheck, Fix, Info)
- Optional: Expandable troubleshooting section
- Optional: Timestamp of error

**Layout:**

- Card-based list (one error = one card)
- Responsive: buttons wrap on mobile
- Error count badge in navigation header

### 3. Notification Toast Patterns

**When to show toasts** (vs. just updating badge):

- ❌ Status changes (connecting/connected): Don't toast (badge is enough)
- ✅ Error detected: May toast if user is actively using app
- ✅ Critical errors: Toast with action buttons (e.g., "Fix", "Dismiss")
- ❌ Recovery/resolution: Don't toast (badge turns green)

### 4. Information Hierarchy

**In errors list:**

1. Connector name (biggest, most important)
2. Error message (secondary, actionable)
3. Help text / Info button (tertiary, on-demand)
4. Troubleshooting guide (hidden, expandable)

### 5. Color Coding

| State          | Hex Color | Usage                         | Emotion           |
| -------------- | --------- | ----------------------------- | ----------------- |
| Connected      | #10B981   | Green badge, streaming icon   | Healthy, ready    |
| Connecting     | #F59E0B   | Yellow badge, loading spinner | Processing, wait  |
| Disconnected   | #6B7280   | Grey badge, dim text          | Neutral, disabled |
| Error          | #EF4444   | Red badge, alert icon         | Urgent, broken    |
| Live (YouTube) | #EF4444   | Red pulsing badge             | Attention, active |

### 6. Accessibility

**Requirements:**

- ARIA labels for status indicators
- Semantic HTML (buttons, not divs)
- Keyboard navigation for error cards
- Error messages not relying on color alone
- High contrast text on colored backgrounds
- Animated dots have reduced-motion support

---

## Implementation Checklist

### Display Components

- [ ] ConnectorStatusBadge: 4-state display with icons/colors
- [ ] NavConnectors: Compact status row in header
- [ ] ConnectorDashboardWidget: Full status + capability flags
- [ ] ErrorList: `/errors` page with cards and actions
- [ ] ErrorCard: Individual error with buttons and info panel
- [ ] ConnectorFixModal: Settings form for connector configuration

### State Management

- [ ] obsStatus, atemStatus, youtubeStatus, etc.: Individual status stores
- [ ] connectorErrors: Error list store with push/clear methods
- [ ] errorCount: Derived count for badge
- [ ] obsDeviceListenerStatuses: Device-specific error tracking

### Actions & Recovery

- [ ] Recheck command mapping (obs → connect_obs, etc.)
- [ ] Fix modal connector detection
- [ ] Error clearing on reconnection
- [ ] Markdown rendering for infoMarkdown field

### Styling

- [ ] CSS variables for status colors
- [ ] Badge styles (compact, with dot, label)
- [ ] Error card layout and spacing
- [ ] Responsive design for mobile
- [ ] Animation for connecting state
- [ ] Live pulsing animation for YouTube LIVE badge

### Messaging

- [ ] User-facing error messages (not technical stack traces)
- [ ] Troubleshooting guides in markdown format (per connector)
- [ ] Localization hooks for error messages
- [ ] Consistent terminology (e.g., "connection failed" vs "unavailable")

### Testing

- [ ] Status transitions (all 4 states)
- [ ] Error card display and actions
- [ ] Recheck command execution
- [ ] Markdown rendering for special characters
- [ ] Responsive layout on small screens
- [ ] Accessibility: keyboard nav, screen reader labels

---

## Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        BACKEND (Rust)                            │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Connectors (OBS, ATEM, YouTube, Discord, Broadlink)     │   │
│  │  - Poll/listen for status                                │   │
│  │  - Detect errors, capture error messages                 │   │
│  │  - Send updates via WebSocket                            │   │
│  └──────────────────────────────────────────────────────────┘   │
└────────────────┬──────────────────────────────────────────┬─────┘
                 │                                          │
                 │ WebSocket Messages                       │ REST API
                 ↓                                          ↓
┌─────────────────────────────────────────────────────────────────┐
│                      FRONTEND (SvelteKit)                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  WebSocket Handler (lib/ws/client.ts)                    │   │
│  │  - Receives connector.status messages                    │   │
│  │  - Updates status stores                                 │   │
│  │  - Pushes errors to error store                          │   │
│  └──────────────────────────────────────────────────────────┘   │
│                          ↓                                       │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  State Stores (Svelte)                                   │   │
│  │  - obsStatus, atemStatus, youtubeStatus, etc.            │   │
│  │  - connectorErrors, errorCount                           │   │
│  │  - Subscription-based updates                            │   │
│  └──────────────────────────────────────────────────────────┘   │
│                          ↓                                       │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  UI Components                                           │   │
│  │  - NavConnectors (header badges)                         │   │
│  │  - ErrorList (/errors page)                              │   │
│  │  - ConnectorDashboardWidget                              │   │
│  │  - ConnectorStatusBadge                                  │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Design Prompt for AI Tool

**For UI/UX Designers or Design AI Tools:**

"Design a comprehensive system status and error management dashboard for a streaming control application managing 8 real-time hardware/software connectors (OBS, vMix, ATEM Switcher, YouTube, Facebook, Discord, Broadlink IR/RF, OBS Device Listeners).

**Requirements:**

1. **Status Indicator System:**
   - 4-state model: connected (green), connecting (yellow), disconnected (grey), error (red)
   - Animated connecting state (pulsing or spinner)
   - Real-time badge updates in navigation bar
   - Color-coded indicator dots
   - Status labels that change with state

2. **Error Management:**
   - Centralized error list page (/errors)
   - Error cards showing: connector name, error message, action buttons
   - Expandable troubleshooting guides (rendered markdown → HTML)
   - Three action types: Recheck, Fix (settings modal), Info
   - Error count indicator in navigation

3. **Connector-Specific Considerations:**
   - Platform connectors (YouTube, Facebook): OAuth/authentication flows
   - Hardware devices (ATEM, Broadlink): Network/IP configuration
   - Software devices (OBS, vMix): Port/password configuration
   - Special cases: Device listeners, RF/IR learning, live detection

4. **Visual Hierarchy:**
   - Quick status overview in navigation (compact badges)
   - Detailed view in dashboard (full cards with flags)
   - Error management on dedicated page (cards + actions)
   - Settings integration for connector configuration

5. **Interaction Patterns:**
   - Clicking status badge → Opens connector dashboard/settings
   - Error notification → Dismissable, links to error detail page
   - Recheck action → Shows loading state, updates status
   - Fix action → Modal form for settings update
   - Info action → Expands troubleshooting guide

6. **Design System:**
   - Semantic color palette (green/yellow/red/grey)
   - Glass morphism cards (matching app aesthetic)
   - Badge components for status display
   - Modal for connector fix/setup
   - Responsive layout (desktop + tablet)
   - Accessibility: ARIA labels, keyboard nav, color contrast

7. **Animations:**
   - Smooth status transitions (0.3s)
   - Pulsing dot for connecting state
   - Pulsing red dot for YouTube LIVE (continuous)
   - Skeleton loader for status checks
   - Toast-style notifications (optional, context-dependent)

8. **Information Architecture:**
   - Navigation: Compact status badges + error count
   - Dashboard: Full connector widgets with controls
   - Errors page: Prioritized error list with recovery options
   - Settings: Per-connector configuration forms
   - Help: Inline tooltips, expandable guides

**Deliverables:**

- Wireframes for each component
- Color specifications and accessibility checks
- Animation sequences (status transitions, pulsing)
- Responsive breakpoints (mobile/tablet/desktop)
- Component library documentation
- Accessibility audit (WCAG 2.1 AA compliance)"
