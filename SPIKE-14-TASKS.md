# Spike 14 tasks (deleted when the spike closes)

The issue is #48. Status: `[ ]` open, `[x]` done.

The notes come from the Mutopia Project's edition (public domain, Mutopia-2014/12/14-37). The
reference piano is the Salamander Grand Piano V3 by Alexander Holm (CC BY 3.0), as FreePats'
SF2, played by FluidSynth. Neither is in the repo: both are in `~/.cache/wrela`.

## 1. The score (`engine::score`)
- [x] Notation text read in a constant: pitches, durations, dots, ties, rests, chords, bars
- [x] Marks: dynamics, hairpins, slurs, accents, staccato, tenuto, fermatas, pedal
- [x] Voices of a passage together, checked: every bar fills its meter, the voices agree
- [x] Form as code: passages one after another (the repeat and its two endings)
- [x] Errors fail the build with the voice, bar and beat; tests for each (suite/piano.rs)

## 2. The performance (`engine::perform`)
- [x] Each note's onset (s), velocity and release; pedal and una corda curves
- [x] The deadpan: no taste, no marks, exactly the score
- [x] Taste: tempo, phrase arch, final ritardando, melody lead, voicing, dynamics' range,
      metre, legato, pedal by a voice's onsets, small random spread (seeded), final ring
- [x] Marks: lean, breathe, drag, roll, rubato in one voice, hold, swell
- [x] Ornaments and breaths in clock time
- [x] Computed at load, so its literals lift (45 of performance.wrela's, none left out)

## 3. The piano (`engine::piano`)
- [x] Partials with inharmonicity and the model's tuning
- [x] Two decays per partial with a small detune (beating, double decay)
- [x] Hammer: levels by key, velocity and frequency from the model; strike position; knock
- [x] Dampers on release (none in the top octaves); sustain pedal with half-pedal; una corda
- [x] Sympathetic resonance with the pedal down
- [x] Soundboard and room (stereo)
- [x] vec4 resonator banks, so the bank's arithmetic is SIMD
- [x] The model fitted to the reference (`wrela audio model`, closed loop): partials within
      2.6 dB on average over 80 notes, the loudest within 1.1 dB

## 4. Stereo voice (std, runtime, spec)
- [x] `std::audio`: two channels; abi memory, worklet, offline test page, native render
- [x] language.md §6.13; the audio tests

## 5. The piece (`examples/gymnopedie`)
- [x] All 78 bars from the Mutopia edition, the repeat written as form
- [x] A taste and marks written in code; every f32 lifted
- [x] Both hosts give the same samples (the whole piece, bit for bit)
- [x] Chrome renders it at 75× real time, and plays it live with no underrun

## 6. Tools (`wrela audio`)
- [x] `wav`, `describe`, `numbers`, `midi`, `sheet`, `speed`, `partials`, `model`
- [x] `tools/listening.py`: the four blind renders and their key

## 7. Results (posted on #48)
- [x] Claude's first performance from the tools alone
- [ ] The owner's listening: rankings, notes, and rounds of the performance after them
- [x] Sizes: the score and performance as built, the piano's code, against 128 kbps audio
- [x] `tools/check.sh` passes (the lens's studio-build speed budget is noisy under load: over
      once in three runs, on a different subject each time)
- [ ] Commit on `spike-14`; post the results on #48
