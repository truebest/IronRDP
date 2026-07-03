# third_party/IronRDP — vendored, patched IronRDP

This is a **durable in-repo vendored copy** of [Devolutions/IronRDP](https://github.com/Devolutions/IronRDP),
patched for gnome-remote-desktop. It replaces the previous ephemeral `/tmp/IronRDP`
checkout that `webrdp-min/Cargo.toml` used to point at, so the build no longer
depends on anything outside this repository.

## Upstream base

- Repo: `https://github.com/Devolutions/IronRDP`
- Base commit: `90461444f994e68847c38f7f1c27d21fe95f2839`
- Workspace edition: 2024 (toolchain pinned `1.89.0` in `rust-toolchain.toml`;
  webrdp-min builds with the repo's rustup default ≥1.85 and does not depend on
  this pin, since cargo is invoked from `webrdp-min/`).

## Local delta (the gnome-remote-desktop patch)

The working-tree changes over the base commit are captured, byte-for-byte, in
`../../patches/ironrdp/0001-gnome-rdp-support.patch` (the canonical provenance
record). They touch only:

- `crates/ironrdp-connector/{Cargo.toml,src/connection.rs,src/connection_activation.rs,src/lib.rs}`
  — CredSSP made an optional feature (`default = ["credssp"]`); DeactivateAll→DemandActive
  reactivation tolerance; `SUPPORT_DYN_VC_GFX_PROTOCOL` early-capability bit;
  network-characteristics autodetection support: `SUPPORT_NET_CHAR_AUTODETECT` +
  `ClientMessageChannelData` GCC blocks, the server-granted MCS message channel captured
  into `ConnectionResult.message_channel_id` (and joined when channel-join isn't skipped),
  and the previously pass-through `ConnectTimeAutoDetection` connector state now answers
  connect-time RTT/bandwidth-measure requests on the message channel until licensing
  starts — FreeRDP-based servers (gnome-remote-desktop) otherwise disable audio output
  redirection, and once the client advertises the capability the server blocks the
  connection waiting for these responses.
- `crates/ironrdp-session/{src/x224/mod.rs,src/active_stage.rs}` — the x224 `Processor`
  learns `message_channel_id` and answers continuous autodetect RTT requests arriving on
  the MCS message channel during the active session (gnome-remote-desktop pings these to
  estimate audio render latency); `crates/ironrdp-testsuite-core/tests/session/autodetect.rs`
  updated for the new `Processor::new` arity.
- `crates/ironrdp-session/Cargo.toml` — connector consumed with `default-features = false`.
- `crates/ironrdp-egfx/src/client.rs`, `crates/ironrdp-graphics/src/progressive.rs`
  — `WireToSurface2` RemoteFX-Progressive decode → RGBA tiles; a
  `GraphicsPipelineHandler::on_map_surface_to_output` hook (the base dispatcher previously
  consumed `RDPGFX_MAP_SURFACE_TO_OUTPUT_PDU` internally without exposing it, so a mapped
  surface's origin couldn't be applied to its bitmap updates); `BitmapUpdate` is no longer
  `#[non_exhaustive]` so the sole downstream consumer (`webrdp-min`) can construct it in tests.
  A progressive decode failure in `handle_wire_to_surface2` now propagates as a `PduResult`
  error (matching `decode_avc420`'s behavior) instead of being logged and silently dropped,
  which used to leave the session `Active` with a black/stale screen and no error reported.
  `handle_reset_graphics`/`DeleteEncodingContext` now call `ProgressiveDecoder::reset()`/
  `delete_context()` (already present on the decoder but never wired up), so stale per-context
  tile state can't survive a graphics reset or an explicit context deletion. `ProgressiveDecodeError`
  gained `impl core::error::Error` so it can be attached as a `PduError` source.
- `crates/ironrdp-web/{Cargo.toml,src/session.rs}` — upstream reference path (not built
  by `webrdp-min`; kept for parity/provenance).

To regenerate the patch from this tree against the base commit, or to re-apply it on
a fresh upstream checkout, use `../../patches/ironrdp/0001-gnome-rdp-support.patch` as the canonical patch record.

## What was trimmed from the upstream tree

To keep the vendored copy lean, these non-essential upstream paths were **not** copied
(they are not needed to build the protocol crates `webrdp-min` depends on):
`web-client/`, `ffi/`, `benches/`, `xtask/`, `fuzz/`, `.git/`, `.github/`, and the
upstream `CLAUDE.md`/`AGENTS.md` agent manuals. The workspace `members` list in
`Cargo.toml` was reduced to `["crates/*"]` accordingly. All `crates/*` are kept intact
so workspace inheritance (`workspace.package` / `workspace.dependencies` /
`workspace.lints`) resolves exactly as upstream.

## Remaining (optional) fork/push step

This vendored copy is fully self-contained and needs no remote. If a GitHub fork is
later desired (e.g. to track upstream via a submodule instead of a vendored tree):

1. `gh repo fork Devolutions/IronRDP` (or create a new repo), check out base commit
   `9046144`, `git apply patches/ironrdp/0001-gnome-rdp-support.patch`, commit on a
   `gnome-rdp-support` branch, and push.
2. Replace this directory with `git submodule add <fork-url> third_party/IronRDP`
   pinned to that commit, and keep `webrdp-min/Cargo.toml` paths unchanged
   (`../third_party/IronRDP/crates/*`).

Until then, the vendored tree IS the durable source of truth.
