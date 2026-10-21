# UI control audit

Method: Eden was built (`cargo build --locked`) and driven through real mouse/keyboard input on an Xvfb X11 display with the SDL dummy audio driver, then captured at 1280x800. Controls were clicked with a held press (press and release in separate frames); an instantaneous press/release does not register on some buttons and is a harness limitation, not an Eden defect. This is a visual and input check, not a listening or latency check.

Reviewed so far: transport/toolbar buttons, Add Track menu and all three track types, double-click clip creation for MIDI/audio/automation, the three clip editors in the bottom Edit panel, the FILES/CLIPS/MOD/THEME tabs, and the MIXER/EDIT/RACK/MASTER tabs. Not yet reviewed: Options popup, piano-roll tools, automation-editor tools, context menus, small window sizes and larger font scales.

## Fixed

| Trigger | Fix |
|---|---|
| Audio editor on an audio clip with no source file: NORM, TRIM, CUT, PASTE, Export and APPLY looked enabled and did nothing when clicked or triggered by W/E/T/Y/B. | These buttons are disabled with the reason as the hover hint ("This clip has no audio file to edit", or "Audio file not found: name"). Using the keyboard shortcut shows the same reason as a toast. |
| Tooltips were drawn above the pointer and covered the menu row or tab above the hovered control. | Tooltips sit below the pointer and flip above only when there is no room; placement is a tested pure function. |
| TRIM or CUT with no selection did nothing. | A toast explains that a selection is needed (drag across the waveform, or SEL). |
| NORM, TRIM, CUT, PASTE and APPLY silently did nothing when the source file could not be decoded. | A toast names the file and the decode error. |

Regression tests: `views::audio_editor::tests` cover the reason text for empty, missing and existing sources and the decode-failure toast. The button input path was verified on the running app (disabled rendering, hover reason, keyboard toast).

## Open, with triggers and acceptance criteria

1. **Tooltip over open menus.** The tooltip still appears while the Add Track menu is open. It no longer covers the rows above, but it can cover the row below the pointer. Acceptance: no tooltip while a menu is open. Re-verify visually; only the placement function is unit tested.
2. **New clips are all named "Clip".** Trigger: double-click an empty lane on three tracks. The CLIPS library then lists three entries named "Clip". Acceptance: new clips get distinct names, or the library distinguishes them without relying on the name. This needs a naming decision and ties into the clip-identity migration in [clip-editing.md](clip-editing.md).
3. **Clips created by double-click stay selected together.** Trigger: create a clip on each of three tracks; all three show selection handles. Acceptance: creating a clip selects only that clip. Needs reproduction in a unit test before changing.
4. **Truncated labels.** Trigger: select an automation clip; the editor header reads "AUTO: volum". MIDI editor header text "MIDI 1 / Clip (1b DRAW" is also cut off at the default size. Mixer strip titles truncate ("Audio" for "Audio 1"). Acceptance: labels fit or are shortened deliberately with the full text in a hover hint.
5. **Options popup at XL font, light-theme track titles, and the full Edit-page sidebar overlap** remain as recorded in [ui-controls.md](ui-controls.md) and [clip-editing.md](clip-editing.md). The Edit tab inspected here is the bottom panel; the separate full-page sidebar overlap was not reproduced in this pass.
6. **Empty mixer panel.** Trigger: new project with no tracks, MIXER tab selected; most of the panel is empty. Acceptance: an empty state message or a collapsed panel.
