# Installing Metocast on an iPhone or iPad

The iPhone and iPad app (`apple-app/Metocast.xcodeproj`, bundle ID `com.metocast.native`, iOS 26
or later) is a client of the Metocast server running on a Mac. There are two ways to put it on a
device:

| Route | Account | How long it runs | For services? |
| --- | --- | --- | --- |
| **TestFlight** (below) | Apple Developer Program (paid) | each build 90 days; upload a new one before then | yes |
| **Xcode, Run on the device** | Apple Developer Program (paid) | 1 year (the profile's life) | yes, as a fallback |
| **Xcode, Run on the device** | free Personal Team | **7 days**, then the app stops opening | no |

A free Personal Team build stops launching after a week, possibly mid-service, so use it only to
try the app.

## Once: signing and App Store Connect

1. In Xcode → Settings → Accounts, sign in with the Apple ID of the Developer Program team.
2. Open `apple-app/Metocast.xcodeproj`, select the **Metocast** target → Signing & Capabilities,
   and pick the team. Signing stays **Automatic**; Xcode writes the team ID into the project as
   `DEVELOPMENT_TEAM`.
3. In [App Store Connect](https://appstoreconnect.apple.com) → Apps → **+** → New App: platform
   iOS, bundle ID `com.metocast.native` (register it under Certificates, Identifiers & Profiles
   first if it is not in the list). The app never needs to be submitted for review: internal
   TestFlight testers install builds straight away.
4. App Store Connect → the app → TestFlight → Internal Testing: add a group and add yourself (and
   anyone else who runs the services) as testers.
5. Install **TestFlight** from the App Store on the iPhone and the iPad and sign in with a tester's
   Apple ID.

## Each release: archive and upload

The version on the device follows the release tags: the marketing version is the latest `v*` tag
(as CI stamps the Mac app), and the build number is the upload time, which App Store Connect needs
to be new for every upload. Nothing in the project file changes, so there is nothing to commit.

Build the web pages and the bridge first, as for any build (see the README), then:

```bash
TAG=$(git describe --tags --abbrev=0 --match 'v*')
xcodebuild -project apple-app/Metocast.xcodeproj -scheme Metocast -configuration Release \
  -destination 'generic/platform=iOS' -archivePath target/Metocast-iOS.xcarchive \
  MARKETING_VERSION="${TAG#v}" CURRENT_PROJECT_VERSION="$(date +%Y%m%d.%H%M)" \
  -allowProvisioningUpdates archive
xcodebuild -exportArchive -archivePath target/Metocast-iOS.xcarchive \
  -exportOptionsPlist apple-app/ExportOptions.plist -exportPath target/Metocast-iOS-export \
  -allowProvisioningUpdates
```

[`ExportOptions.plist`](../../apple-app/ExportOptions.plist) exports for App Store Connect and
uploads in the same step, signed with the Xcode account from step 1. The same can be done by hand:
Xcode → Product → Archive (with "Any iOS Device" as the destination; set Version and Build on the
target's General tab first), then Organizer → Distribute App → **TestFlight Internal Only**.

After a few minutes of processing the build shows in TestFlight on each tester's devices; tap
**Install** (or turn on automatic updates in TestFlight). The app declares that it uses no
non-exempt encryption, so no export-compliance question holds the build back.

## Xcode direct install

Connect the device by cable (or pair it over Wi-Fi in Window → Devices and Simulators), turn on
Developer Mode on it (Settings → Privacy & Security), choose it as the run destination and press
Run. A paid team's build runs for a year; a Personal Team's for 7 days (see the table above).

## Connecting to the Mac server

On the Mac that runs the server, open Settings (the toolbar's Settings button, or Metocast →
Settings…). While the server runs it shows the address, the token and a **Connect a Device** QR
code.

- **QR code:** point the iPhone or iPad Camera app at it and tap the Metocast banner. The
  `metocast://connect` link carries the address and token, and the app connects.
- **Address and token:** on the app's setup screen type the server address (for example
  `192.168.1.10:3737` or `the-mac.local:3737`) and the token, both shown in the Mac's Settings.

The device must be on the same network as the Mac. The first time, iOS asks for Local Network
access: allow it, or the app cannot reach the server (Settings → Privacy & Security → Local
Network → Metocast turns it back on).
