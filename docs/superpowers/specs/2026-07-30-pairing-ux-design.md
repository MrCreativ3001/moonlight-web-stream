# Pairing UX overhaul — design

Date: 2026-07-30. Approved by Joey before implementation.

## Problem

Pairing failures surface as a bare `"PairError"` string. Users cannot tell a wrong
PIN from a timeout from a concurrent-attempt collision (issues #143, #26, #19 are
all this wall). Additional hazards discovered while diagnosing a live setup:

- The pairing window is 5 minutes, but the UI never says so; a PIN entered after
  expiry fails with no explanation.
- A second pair attempt while one is pending corrupts Sunshine's pending pair
  session (Sunshine keys sessions by client uniqueid and `emplace` never
  refreshes an existing entry; a stale/raced PIN leaves the session mid-phase and
  every later attempt fails with "Out of order call to getservercert" until
  Sunshine restarts).
- `src/app/host.rs` hardcodes the device name `"roth"` instead of using the
  `moonlight.pair_device_name` config value (separate small PR).

## Changes

### Protocol (`common/src/api_bindings.rs`, ts-rs exported)

- `PostPairResponse1::Pin(String)` → `Pin { pin: String, expires_in_secs: u64 }`.
- `PostPairResponse2` gains `PairFailed { reason: PairFailReason, detail: Option<String> }`.
- New `PairFailReason` enum: `PinIncorrect | TimedOut | AlreadyPaired |
  PairingInProgress | HostUnreachable | Internal`.
- Legacy `PairError` variants stay for old clients; the shipped web client
  upgrades with the server, third-party consumers keep working.

### Server (`src/api/host.rs`, `src/app/host.rs`)

- Map the `AppError`/`MoonlightClientError` chain to `PairFailReason` + detail
  string instead of discarding it in a `warn!`.
- Per-host in-flight guard: a concurrent `POST /pair` for the same host returns
  `PairFailed { reason: PairingInProgress }` immediately. Rejecting (rather than
  cancel-and-replace) is deliberate: racing attempts poison Sunshine's pending
  session (see above).
- `POST /pair/cancel { host_id }`: aborts the in-flight attempt (drops the pair
  future, sends `/unpair` best-effort, clears the guard) so a lost modal doesn't
  lock the host for 5 minutes.

### Client (`web/component/host/`, `web/locales/*.ts`)

- Pair modal: PIN + live countdown driven by `expires_in_secs`; Cancel button
  wired to `/pair/cancel`.
- Human, localized message per `PairFailReason` in all 5 locales (en, fr-FR,
  ko-KR, pt-BR, zh-CN). `PinIncorrect` message includes the stale-session hint:
  restart Sunshine if retries keep failing.

## Testing

- Rust unit tests for error → reason mapping and the in-flight guard.
- Live e2e against Sunshine 2025.924: happy path, wrong PIN, expiry, duplicate
  attempt, cancel-then-retry.

## Out of scope

Sunshine's stale-session bug itself (filed upstream at LizardByte separately);
latency work (#136); zombie streams (#98); sidebar/env fixes (#155/#148).
