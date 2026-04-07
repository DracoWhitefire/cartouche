# Testing strategy

Encode/decode libraries benefit from a testing approach that combines small deterministic
tests with corpus-based fuzz validation.

## Test categories

### Unit tests

Unit tests cover narrow pieces of logic and live next to the code they test. Encode tests
construct a typed struct, call `into_packets()`, and assert on the resulting byte array.
Decode tests take a handcrafted `[u8; 31]` and assert on the decoded fields and any
warnings.

Round-trip tests are the primary correctness signal: construct a frame, encode to packets,
decode from packets, assert that the decoded frame equals the original.

This keeps failures localised: a failing test in `avi.rs` can only mean the AVI encode or
decode path is broken.

### Warning coverage

Each test file should include tests for every warning condition its InfoFrame type can
produce:

- `ChecksumMismatch`: corrupt byte 3 of an otherwise valid packet.
- `ReservedFieldNonZero`: set a reserved bit in the payload.
- `UnknownEnumValue`: set a field to a value not present in the spec-defined enum.

These tests verify that the frame is still returned (not errored) and that exactly the
expected warning is attached.

### `DecodeError::Truncated`

Each decode test file should include a test that sets `length > 27` in the packet header
and asserts that `decode` returns `Err(DecodeError::Truncated { .. })`.

### Fuzzing

Fuzzing is strongly recommended for all decode paths.

Two invariants must hold for any 31-byte input:

1. **No panic** — `decode` must never panic. It either returns `Ok(Decoded { .. })` (with
   zero or more warnings) or `Err(DecodeError::Truncated { .. })`. No other outcome is
   acceptable.
2. **Round-trip identity** — for any well-formed frame, `decode(frame.into_packets().next())
   == frame` (modulo checksum, which is always recomputed on encode).

Five fuzz targets live in `fuzz/fuzz_targets/`, one per InfoFrame type:

- `avi` — exercises `AviInfoFrame::decode`
- `audio` — exercises `AudioInfoFrame::decode`
- `hdr_static` — exercises `HdrStaticInfoFrame::decode`
- `hdmi_forum_vsi` — exercises `HdmiForumVsi::decode`
- `dynamic_hdr` — exercises `DynamicHdrInfoFrame::decode_sequence`

All targets are set up using `cargo-fuzz` with libFuzzer. `cargo-fuzz` requires nightly;
the library itself stays on stable.

#### Quick smoke run

Runs until Ctrl+C. Useful for interactive testing:

```
cargo +nightly fuzz run avi
cargo +nightly fuzz run audio
```

Any crashes are written to `fuzz/artifacts/<target>/`.

#### Long campaign

Run for a fixed duration (e.g. one hour), then minimise and commit the resulting corpus:

```
cargo +nightly fuzz run avi -- -max_total_time=3600
cargo +nightly fuzz cmin avi
```

The minimised corpus lives in `fuzz/corpus/<target>/` and should be committed so that
subsequent runs — locally and in CI — start from a richer base.

#### CI

The fuzz workflow (`.github/workflows/fuzz.yml`) runs a 60-second smoke test on every
push and pull request, and a 1-hour deep run on a weekly schedule and on manual dispatch.
The corpus is cached between runs. Crash artifacts are uploaded automatically on failure.

After each deep run, each target uploads its minimised corpus as a workflow artifact. A
final `update-corpus` job assembles all corpora and opens a `ci/fuzz-corpus` PR if any
corpus file changed — one PR per deep run covering all five targets.

## Test philosophy

cartouche should be strict about the wire format and permissive about out-of-spec field
values.

The test suite reflects that balance:

- `DecodeError::Truncated` is the only hard failure; everything else returns a frame with
  warnings attached.
- Round-trip tests cover every field variant to confirm that no information is lost or
  corrupted in the encode/decode cycle.
- Warning tests confirm that anomalous inputs are surfaced to the caller rather than
  silently discarded.
