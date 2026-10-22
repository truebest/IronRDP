# lgnome IronRDP fork

Upstream: https://github.com/Devolutions/IronRDP

Base: `e4a0d17b8e5b1b81758202dc131d8fa2aed58a23` (2026-10-23).

Client protocol reset, including the Windows ACK restart and terminal reset errors,
is provided by merged [upstream PR #2207](https://github.com/Devolutions/IronRDP/pull/2207).

The lgnome patch set retains:

- Optional connector CredSSP dependencies; lgnome supplies its own authentication.
- Typed DVC listeners across server-driven channel recreation.
- Hardware AVC420/base-view passthrough and auxiliary-view callbacks.
- Private HEVC passthrough: `FRDP_1` (`0x00010000`), flag `0x40000000`,
  codec ID `0x0002`, AVC420 metablock followed by Annex-B. HEVC payloads require
  capability confirmation; no software HEVC decoder is included.
- Compositor opt-out for direct native presentation, preserved across graphics resets.
- Partial Progressive output, preserved codec state across dropped updates, and bounded
  undecodable-stream reporting.
- Optional frame-acknowledgment suspension, rearmed after a graphics reset.
- Typed server termination reasons during activation and active sessions.
- The screenshot diagnostic's graphics probes.
- Bounded incoming virtual-channel reassembly: 16 MiB for ordinary DVC messages,
  64 MiB for EGFX messages and SVC sequences, with checks before retaining fragments.
  The capture replay graphics wrapper forwards the EGFX budget.

Upstream now provides REGION clipping, the Windows Progressive context/quality fixes,
Windows SRL decoding, and configurable early EGFX capability advertisement.
The fork uses those implementations.

Reassembly limits also apply to decompressed DVC messages while retaining the upstream
per-channel RDP 8.0 Lite compression history.

Progressive decode failures preserve tile contexts on every surface. The client retains
its cumulative, rate-limited failure logging and reports sustained undecodable streams
through the existing handler callback without resetting shared decoder state. Protocol
context and surface deletion continue to own that state lifetime.

License: MIT OR Apache-2.0; see LICENSE-MIT and LICENSE-APACHE.
