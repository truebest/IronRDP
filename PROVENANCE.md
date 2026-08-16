# third_party/IronRDP — gnomecast IronRDP fork

This is gnomecast's **fork of [Devolutions/IronRDP](https://github.com/Devolutions/IronRDP)**
(https://github.com/truebest/IronRDP, branch `gnome-rdp-support`), patched for
gnome-remote-desktop and consumed by the gnomecast repository as a git submodule at
`third_party/IronRDP`. It was previously vendored in-repo; the fork carries the full
upstream history up to the base commit plus the gnomecast delta on top.

## Upstream base

- Repo: `https://github.com/Devolutions/IronRDP`
- Base commit: `c0a29813bfbdf5db54e8f0dbb7c4ad12a3d83c16`
- Workspace edition: 2024 (toolchain pinned `1.89.0` in `rust-toolchain.toml`;
  webrdp-min builds with the repo's rustup default ≥1.85 and does not depend on
  this pin, since cargo is invoked from `webrdp-min/`).

## Local delta (the gnome-remote-desktop patch)

The git history on `gnome-rdp-support` is the canonical provenance record — the delta
is small enough now that a separate mirrored `.patch` file isn't kept. As of the base
commit above, upstream has independently absorbed the SUPPORT_NET_CHAR_AUTODETECT /
MCS-message-channel / connect-time-RTT autodetect work this fork originally added
(`ironrdp-connector::connection.rs`'s state machine, `ironrdp-session`'s x224 `Processor`
and `ActiveStage`, and the autodetect testsuite are now identical to upstream). What
remains local:

- `crates/ironrdp-connector/src/lib.rs` — CredSSP gated behind a `credssp` Cargo feature
  (`sspi`/`picky*` become optional deps) so `webrdp-min` can build the connector without
  pulling in Kerberos/NLA.
- `crates/ironrdp-connector/src/connection.rs` — on top of upstream's RTT-only
  connect-time autodetect response, this fork also answers Bandwidth-Measure-Start/
  Payload/Stop with real timing (`process_connect_time_autodetect`), and keeps
  `SUPPORT_DYN_VC_GFX_PROTOCOL` in the early-capability flags so FreeRDP-based servers
  (gnome-remote-desktop) grant the Graphics Pipeline (EGFX). The connector config also
  exposes `enable_audio_capture`; when set, Client Info carries `INFO_AUDIOCAPTURE` so
  gnome-remote-desktop can open the MS-RDPEAI `AUDIO_INPUT` DVC. Other in-tree config
  constructors explicitly default it off.
- `crates/ironrdp-connector/src/connection_activation.rs` — broadens upstream's
  DeactivateAll-only tolerance during Capabilities Exchange to skip any non-DemandActive
  Share Control PDU (gnome-remote-desktop interleaves more than just DeactivateAll here).
- `crates/ironrdp-session/Cargo.toml` — connector consumed with `default-features = false`.
- `crates/ironrdp-session/src/{fast_path.rs,image.rs}` — selectively backports upstream
  commit `80bb81b344dba0197aa7b870c685f398dc4bcaee` so decoded bitmap source stride and
  row order remain independent of the visible destination rectangle. This prevents padded
  RDP6 rows from producing diagonal striping on xrdp's login screen.
- `crates/ironrdp-egfx/src/client.rs`, `crates/ironrdp-graphics/src/progressive.rs`
  — `WireToSurface2` RemoteFX-Progressive decode → RGBA tiles; `BitmapUpdate` is no longer
  `#[non_exhaustive]` so the sole downstream consumer (`webrdp-min`) can construct it in tests.
  The EGFX client also selectively backports upstream commit `66c8a81be0a9f966e3cf4935ca2a0274d10b063f`
  to decode `WireToSurface1` RDP 6.0 Planar bitmaps through the existing RDP6 decoder and
  publish them as RGBA updates; this is a focused backport, not a rebase onto that upstream
  revision.
  A progressive decode failure in `handle_wire_to_surface2` now propagates as a `PduResult`
  error (matching `decode_avc420`'s behavior) instead of being logged and silently dropped,
  which used to leave the session `Active` with a black/stale screen and no error reported.
  `handle_reset_graphics`/`DeleteEncodingContext` now call `ProgressiveDecoder::reset()`/
  `delete_context()` (already present on the decoder but never wired up), so stale per-context
  tile state can't survive a graphics reset or an explicit context deletion. `ProgressiveDecodeError`
  gained `impl core::error::Error` so it can be attached as a `PduError` source.
- `crates/ironrdp-web/{Cargo.toml,src/session.rs}` — upstream reference path (not built
  by `webrdp-min`; kept for parity/provenance).
- `crates/ironrdp-displaycontrol/src/client.rs` — `DisplayControlClient::process()` decodes
  the full headered `DISPLAYCONTROL_CAPS_PDU` (MS-RDPEDISP 2.2.2.1) instead of the raw
  capability body; the raw decode read the header's Type/Length as caps fields, so
  `max_monitor_area()` came out tiny and callback-produced monitor layouts were never sent.
  Upstream master has the same bug (its own `DisplayControlServer` and testsuite golden
  vectors prove the headered shape) — upstream PR candidate.
- `crates/ironrdp-dvc/src/client.rs` (+ testsuite `tests/dvc/client_listener.rs`) —
  `with_typed_listener`/`attach_typed_listener`: listener registration that keeps
  `get_dvc_by_type_id` working across server-driven DVC close/re-create cycles.
  Upstream's `with_dynamic_channel` consumes its processor on the first
  DYNVC_CREATE_REQ, so a channel the server closes and re-creates gets NO_LISTENER
  and typed lookup goes permanently dead (defensive hardening; the testsuite
  documents both behaviors). Upstream PR candidate.

To see the delta against upstream directly, diff this tree against the base commit above
(`git diff <base-commit> HEAD -- crates/`).

## What was trimmed from the upstream tree

To keep the fork lean, these non-essential upstream paths were removed
(they are not needed to build the protocol crates `webrdp-min` depends on):
`web-client/`, `ffi/`, `benches/`, `xtask/`, `fuzz/`, `.git/`, `.github/`, and the
upstream `CLAUDE.md`/`AGENTS.md` agent manuals. The workspace `members` list in
`Cargo.toml` was reduced to `["crates/*"]` accordingly. All `crates/*` are kept intact
so workspace inheritance (`workspace.package` / `workspace.dependencies` /
`workspace.lints`) resolves exactly as upstream.

## Updating from upstream

Rebase `gnome-rdp-support` onto a newer upstream commit, resolve conflicts in the files
listed above (checking whether upstream has absorbed any of this fork's delta, as it did
for the autodetect/message-channel work), update the base commit above, then bump the
submodule pin in the gnomecast repository.
