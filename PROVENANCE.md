# lgnome IronRDP fork

Upstream: https://github.com/Devolutions/IronRDP

Base: `1bec1d57f446a7ddef8f26b0c3c644059564cdc2` (2026-09-06).

The lgnome patch set retains:

- Optional connector CredSSP dependencies; lgnome supplies its own authentication.
- Typed DVC listeners across server-driven channel recreation.
- Hardware AVC420/base-view passthrough and auxiliary-view callbacks.
- Compositor opt-out for direct native presentation.
- Partial Progressive output, bounded recovery, and undecodable-stream reporting.
- Windows SRL decoding fixes and optional frame-acknowledgment suspension.
- The screenshot diagnostic's graphics probes.

Upstream now provides REGION clipping, the Windows Progressive context/quality fixes,
and configurable early EGFX capability advertisement. The fork uses those implementations.

License: MIT OR Apache-2.0; see LICENSE-MIT and LICENSE-APACHE.
