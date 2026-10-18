# Working on Eden

Eden is a Rust/SDL2 desktop DAW primarily operated with mouse and keyboard. Read README.md and the affected code before editing.
Use the existing checkout; cloud tasks are isolated. Do not create a Git worktree unless requested.

## Desktop interaction

- Design and validate pointer hit targets, hover feedback, drag/drop, context menus, keyboard focus, text entry and discoverable shortcuts.
- Editing shortcuts must respect the focused panel and active text fields; toolbar and keyboard actions must share behavior.
- Do not import controller/gamepad navigation, button mappings or controller-first interaction requirements from Mox. Adapt only its general engineering and UI-quality guidance after reading the actual source instructions.

## Product behavior

- Preserve DAW conventions: MIDI and musical automation stay on their musical beat positions when tempo changes. Distinguish musical-time and absolute-time audio explicitly.
- Keep arrangement beats, tempo-map seconds, source sample frames, timeline zoom and UI scale separate. Zoom/UI scale must never change project timing or audio.
- Trim, slip, loop, move, duplicate and stretch are different operations. Never present trim as stretch. Specify pitch behavior for audio stretch.
- Calculate drag previews from an immutable pointer-down baseline. A gesture commits one undoable transaction; Escape cancels. Validate entire groups and preserve their relative spacing.
- Use stable identities for clips/devices/assets when changing ownership. Keep edits non-destructive and preserve old-project compatibility and sound.
- Audio callbacks must not perform file I/O, unbounded allocation, blocking synchronization or last-owner cleanup of large assets. Decode/transform media away from the audio thread.

## UI quality is part of completing every task

- Fix UI defects noticed while working: overlapping text, cut-off labels, clipped or unreachable controls, content that does not fit, broken hit targets and inactive-looking/active-looking state errors.
- Every enabled button must perform its labeled action and expose failures. Unsupported features must be visibly disabled with a specific explanation, or omitted; never add a fake toggle as a substitute for the feature.
- Test affected screens at small and default window sizes and relevant UI/font scales. Reflow, scroll or collapse content instead of allowing overlap. Preserve readable text and clear units.
- Favor a compact mixer and a selected-track device chain. Detailed channel-strip editing belongs in the device view. Preserve DSP behavior when moving controls.
- Review real screenshots after layout changes. Separate actual captures from proposed designs.
- If an observed issue requires a larger unrelated change, record its exact trigger and acceptance criteria and explicitly report what remains; do not silently label it fixed.

## Validation

- Use cargo build --locked and cargo test --locked; keep Cargo.lock stable unless dependency changes are requested.
- Add focused regression coverage for behavior bugs, especially drag-frame sequences, undo/redo, tempo conversion, stereo handling and save/reopen compatibility.
- Verify buttons through their UI input path, not only command classes. A PID or a zero-test run is insufficient.
- Check git status before and after. Preserve user edits and keep generated screenshots, logs and local toolchain files outside tracked source.
- Report observed results, unresolved issues and hardware/environment limits accurately. Do not claim listening or latency validation from a silent sink.

## Guidance adapted from Mox Tracker

Adapted from [Mox Tracker AGENTS.md](https://github.com/pyrotek45/mox-tracker/blob/ed7eb0456f711281cccb1c74a0869b675b0effae/AGENTS.md).
The eight-input/controller contract and controller test requirements do not apply.
Read Eden's README.md, docs/tempo-and-clips.md and relevant feature notes before changing behavior.

## Issue branches and owner-controlled merges

- For a new GitHub issue taken from the Issues page, use a dedicated branch from the current default branch. Keep unrelated issues and unfinished migrations separate.
- Explain dependencies on unmerged work and resolve materially ambiguous combinations with the user before changing workflow or saved-data behavior.
- The user controls merges. Do not merge, enable auto-merge or publish directly to the default branch without explicit authorization for that specific change. A request to implement or test is not merge authorization.
- Keep unfinished PRs in draft; do not mark unrelated work ready. When a PR is requested or prepared, present the concrete implementation and validation for review.

## Keep changes focused

- Limit each change to the requested behavior and the code, tests and
  documentation needed to support it. Avoid unrelated refactors, broad
  formatting changes and dependency upgrades.
- If an unrelated problem is discovered, report it separately instead of
  silently expanding the current issue or PR.


## Clarify feature changes before implementation

- Ask the user for clarification before implementing a feature change when its
  intended behavior is not straightforward, essential details are ambiguous, or
  it could conflict with existing features, controls, saved data or requirements.
- Explain the specific ambiguity or conflict and the practical choices. Do not
  silently choose behavior that could surprise the user or alter their workflow.
- Use the current request, repository documentation and tests to resolve routine
  implementation details. Do not re-ask questions already answered in the session.
- Continue independent inspection or preparation while clarification is pending;
  wait for the answer before implementing the dependent behavior.
- Straightforward cleanup that preserves behavior can proceed without a question.


## Audio quality and physical output safety

- Audio reaches real speakers and headphones. Treat unexpected spikes, excessive
  levels, non-finite samples and abrupt discontinuities as correctness defects.
  No change may knowingly introduce clicks, pops, crackles, dropouts or other
  unintended audible artifacts; intentional distortion must remain controlled.
- Use suitable smoothing, ramps or crossfades for discontinuous controls and
  voice/transport transitions, preserving the intended musical character.
  Bound unstable feedback and modulation. Check extreme and changing parameters.
- Prevent NaN/Inf and uncontrolled level growth from reaching device output.
  Preserve fault containment, safe silence and existing output protection.
  Do not claim software alone can guarantee safe physical listening levels:
  hardware gain and speaker/headphone sensitivity are outside the app.
- Keep realtime callbacks free of blocking, locks, allocation, disk/network I/O
  and logging. Keep processing bounded so missed deadlines do not cause dropouts.
- Separate monitoring/output gain from song mixing, DSP, saved project data and
  offline export. A listening-volume control must not change the rendered song.
- For audio changes, add meaningful finite-output, bounds, continuity and
  realtime/offline parity checks as applicable. Test silence, start/stop, rapid
  edits and extreme settings; retain existing regression coverage.
- Perform listening checks when possible and report when they were not possible.
  Do not hide artifacts by weakening assertions or silently changing DSP.


## Research DSP methods before implementing them

- Before adding or materially changing oscillators, anti-aliasing, filters,
  effects, dynamics, mixing, resampling, metering or other non-trivial audio/DSP,
  research suitable methods online. Do not rely solely on remembered formulas.
- Prefer primary and authoritative sources: original papers, standards,
  official technical documentation and established DSP references. Compare
  relevant approaches when quality, stability or realtime cost differs.
- Choose the method for this DAW's actual requirements: sample-rate
  behavior, aliasing, modulation stability, phase/latency, numerical precision,
  headroom, audible artifacts and bounded realtime CPU/memory use.
- For metering, distinguish the quantity and reference being measured (for
  example sample peak, true peak, RMS or loudness), and consult the applicable
  standard when claiming compliance.
- Record source links near the algorithm or in a linked design note, explain
  why the method fits, and document adaptations, units, coefficients, limits
  and tradeoffs. Check licensing before borrowing implementation code.
- Research informs implementation; it does not replace tests or listening.
  Validate the chosen method against relevant numerical, spectral, stability,
  continuity and level-safety requirements.
- If online research is unavailable, explicitly report that limitation rather
  than presenting an unverified algorithm as researched or best practice.
- Pure renames, comments or behavior-preserving mechanical cleanup do not
  require fresh research unless the underlying DSP behavior is in question.


## Make numbers understandable

- Do not introduce unexplained magic numbers. If a number's meaning is not
  clear from the immediate context, give it a descriptive constant or enum name.
- Include units in names where useful: milliseconds, samples, ticks, rows,
  semitones, bytes, and normalized ranges must be distinguishable.
- Name thresholds, timing values, bit masks, protocol values, limits, UI geometry
  and DSP coefficients when their purpose cannot be inferred locally.
- Explain why a non-obvious value was chosen, including its source or derivation.
  Naming a constant `VALUE_160` does not explain it.
- Obvious structural literals such as zero initialization or incrementing an
  index by one do not need artificial constants. Use judgment: the goal is
  understandable code, not replacing every numeral.
- Use an existing authoritative constant instead of duplicating the same rule.
  Tests should use named domain values, while preserving independent expected
  results so they can catch incorrect implementation constants.


## Finish migrations and remove obsolete paths

- When replacing a feature, remove the implementation it supersedes and update
  every caller. Do not leave dead functions, unused flags, unreachable branches,
  abandoned files, stale configuration, commented-out code, or old execution
  paths behind.
- Update associated tests, help text, footer hints and documentation. Remove
  references to behavior or paths that no longer exist.
- Search for old symbols and paths before declaring a migration complete.
  Verify the replacement is actually reached by the application.
- Retain compatibility code only for a concrete supported caller or data format;
  document that reason. Do not keep it merely "just in case."
- For an explicitly staged migration, document the live boundary, the remaining
  legacy behavior and the next removal step. Keep only the parts still needed
  for that stage, and delete each old path as its replacement becomes active.
- Git history preserves old implementations; the working tree should describe
  the current system.


## Document behavior and algorithm provenance

- Write clear names and focused functions. Add useful documentation for public
  interfaces and for non-obvious behavior, invariants, state transitions,
  precedence rules, edge cases and tradeoffs.
- Explain intent and reasoning rather than narrating obvious syntax. Update or
  remove comments when the implementation changes.
- For DSP or other non-trivial algorithms, identify the algorithm and, when
  known, cite the original paper, official reference, implementation or other
  source used. Record meaningful adaptations and numerical assumptions.
- Explain units, parameter ranges and boundary behavior where needed to use an
  algorithm correctly. Derive formulas or coefficients when a reference alone
  would not explain their role.
- Do not invent provenance. If an algorithm is original or its source is
  unknown, say so when relevant.
- Keep documentation beside the code it explains; use feature documents for
  larger designs and link them from the implementation when useful.


## Final verification and reporting

- Reproduce bugs before changing behavior; record the input sequence and distinguish confirmed failures from suspected causes. Add regression coverage that catches the failure.
- Verify feedback through the renderer, including long labels, empty/selected states and overlays. A status field alone is insufficient if the user cannot read it.
- Run cargo build --locked and cargo test --locked, plus affected functional checks. Check GitHub Actions when a PR is involved; distinguish local checks from CI verification.
- Search for superseded symbols, obsolete paths, unexplained numbers and stale comments before declaring a migration complete.
- Distinguish implemented, locally tested, CI-verified, published and merged. Identify remaining work and unavailable visual/listening checks. Do not claim a whole issue fixed when only part was addressed.
