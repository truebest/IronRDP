# lgnome IronRDP fork

Upstream: https://github.com/Devolutions/IronRDP

Base: `1bec1d57f446a7ddef8f26b0c3c644059564cdc2` (2026-09-06).

The lgnome patch set retains:

- Optional connector CredSSP dependencies; lgnome supplies its own authentication.
- Typed DVC listeners across server-driven channel recreation.
- Hardware AVC420/base-view passthrough and auxiliary-view callbacks.
- Compositor opt-out for direct native presentation.
- Partial Progressive output, preserved codec state across dropped updates, and bounded
  undecodable-stream reporting.
- Windows SRL decoding fixes and optional frame-acknowledgment suspension.
- Typed server termination reasons during activation and active sessions.
- The screenshot diagnostic's graphics probes.
- Bounded incoming virtual-channel reassembly: 16 MiB for ordinary DVC messages,
  64 MiB for EGFX messages and SVC sequences, with checks before retaining fragments.
  The capture replay graphics wrapper forwards the EGFX budget.

Upstream now provides REGION clipping, the Windows Progressive context/quality fixes,
and configurable early EGFX capability advertisement. The fork uses those implementations.

For the reassembly fix, upstream `master` was fetched and inspected at
`e4143f26306a6639a889d80c1c484ca69c365a8f` (2026-10-11). Its DVC accumulator
matched the fork and its SVC accumulator still lacked a message budget, so the
limits are a local fork patch; the existing patch set above is retained.

Progressive decode failures preserve tile contexts on every surface. The client retains
its cumulative, rate-limited failure logging and reports sustained undecodable streams
through the existing handler callback without resetting shared decoder state. Protocol
context and surface deletion continue to own that state lifetime.

License: MIT OR Apache-2.0; see LICENSE-MIT and LICENSE-APACHE.
