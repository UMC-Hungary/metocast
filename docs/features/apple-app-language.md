# Hungarian in the Apple app

The Mac and iPhone app speaks English and Hungarian. It uses the language the device runs in, and
each person can choose a different one for Metocast alone. The web UIs (Classic and Sanctum) have
their own language switch; the app uses the same Hungarian words as their catalogue
(`packages/core-client/locales/hu.json`), so both read alike.

## Choose the language

- **Mac:** **System Settings → General → Language & Region → Applications**, then **+**, choose
  **Metocast** and **Magyar**. Quit and reopen Metocast.
- **iPhone:** **Settings → Apps → Metocast → Language → Magyar**.
- With no choice of its own, Metocast follows the device's language list: Hungarian when it comes
  before English, English otherwise.

Dates, times, durations, file sizes and numbers follow the region set on the device (in Hungary:
`2026. okt. 18. 10:00`, `1,2 GB`, decimal comma).

## What stays English

Some words come from somewhere other than the app, so they read the same in either language:

- **Messages from the server.** A refusal sent in the server's own words, notifications the server
  pushes (an OBS source going missing, a failed upload), a connector's status detail (such as an
  OBS connection error), a job queue's status, an automation run's cause and step results, and the
  server and core logs. Where the server sends a code instead of a sentence
  (`rodecaster_not_connected`, `recording_directory_not_found: /path`, `already_live`, …), the app
  shows it in Hungarian wherever it appears: a refused command, a setting the server would not
  save, a connector's status, a device scan, an automation step and the notifications list. A code
  the app doesn't know yet reads as plain words ("Recording filename invalid") rather than the raw
  code. The YouTube/Facebook sign-in states and the problems an automation editor reports before
  saving (a missing name, trigger or step) are Hungarian too.
- **Names from the equipment and the library.** Mixer channel labels, ATEM inputs, deck and folder
  names, event titles and the Bible text.
- **Product and proper names.** OBS, ATEM, RØDECaster, Middle Control, Keynote, YouTube, Facebook,
  Szentírás.eu, and the readings' own names, *Lekció* and *Textus*, in both languages.
- **The output screen.** Slide counters and labels (*Ének*, *1. dia*) are what the congregation
  sees, and are Hungarian whatever language the operator's app uses.

## For developers

- `apple-app/Metocast/Localizable.xcstrings` holds every string the app's own code shows.
  SwiftUI text (`Text("…")`, `Button("…")`, `Label`, alerts, `ContentUnavailableView`, …) is
  extracted by the compiler. A string built in code goes through `String(localized:)`, and a
  helper that takes a title takes a `LocalizedStringKey`, never a `String`, so the literal at the
  call site is extracted too. Counts use the catalogue's plural variations.
- The English labels the Rust core supplies, connector names (`metocast_core::config::CONNECTORS`)
  and connector setting fields (`metocast_core::config::schema`), are manual entries in the same
  catalogue, looked up with `String(localized: String.LocalizationValue(label))`. A new field
  label in the core needs a manual entry there.
- Server error codes go through `ServerCode.text` (`apple-app/Metocast/Model/Records.swift`);
  `Error.metocastMessage` and the connector status detail already use it. A new code the server
  sends to clients gets a case there, with its Hungarian in the catalogue.
- `apple-app/Metocast/InfoPlist.xcstrings` translates the app's name and its permission prompts
  (local network, microphone, controlling Keynote).
- Xcode adds new strings to `Localizable.xcstrings` when the app builds. A change that adds a
  string adds its Hungarian in the same change: an untranslated string shows in English.
- Check a change in Hungarian without changing the Mac's language:

  ```bash
  open -n target/apple-app-derived/Build/Products/Debug/Metocast.app --args -AppleLanguages "(hu)"
  ```

  In the iPhone Simulator, launch with the same arguments (`xcrun simctl launch booted
  <bundle id> -AppleLanguages "(hu)"`).
