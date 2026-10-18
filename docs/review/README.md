# Eden review evidence and repair plan

- [Findings, reproduced defects, completed UI fixes and staged plan](findings-and-plan.md)
- [Tempo, beats, source samples and display design contract](../tempo-and-clips.md)
- [Interactive timing illustration and proposed rack/mixer layout](layout-proposal.html) — download/open locally; this is a design proposal, not implemented UI.
- [Initial functional/source review](initial-review.md)
- [Diagnostic probe output](evidence/probe-results.txt) and [portable runner](probes/run.py)
- [Full local test output: 564 passed](evidence/full-tests.log)
- [Final targeted toolbar checks](evidence/toolbar-tests.log)

## Actual captures

| Screen | Before | After |
|---|---|---|
| Mixer | [Full channel strips](screenshots/12-mixer.png) | [Compact default strips](screenshots/after-mixer.png) |
| Rack | [Original rack](screenshots/13-rack.png) | [Improved fit](screenshots/after-rack-default.png) |
| Small window | — | [Toolbar and mixer](screenshots/after-small.png), [rack](screenshots/after-rack-small.png) |

The full clip/audio/tempo redesign is not implemented by this change. Physical audio quality and latency were not tested. Screenshots use a virtual display; playback used a silent ALSA sink. Setup caches and generated binaries are intentionally excluded.
