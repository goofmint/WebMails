# SP2 — Data store isolation spike

## Purpose

Verify that two independently-profiled child webviews, both loading `https://www.icloud.com/`,
can hold two different signed-in iCloud sessions side by side and survive an app restart,
before the `profile` component (design §2.2.3) relies on this.

## Question (design §6.3, SP2)

> Do two `data_store_identifier` stores (two iCloud logins) run side by side without crashing?

## Environment

| Field                        | macOS                            | Windows                     |
| ---------------------------- | -------------------------------- | --------------------------- |
| OS build                     | _(fill in — macOS 14+ required)_ | _(fill in — Windows 10/11)_ |
| CPU                          | _(fill in)_                      | _(fill in)_                 |
| Display scale factor         | _(fill in)_                      | _(fill in)_                 |
| `tauri` version (Cargo.lock) | 2.11.6                           | 2.11.6                      |
| `wry` version (Cargo.lock)   | 0.55.1                           | 0.55.1                      |
| `uuid` version (Cargo.lock)  | 1.26.1 (feature `v5`)            | 1.26.1 (feature `v5`)       |
| WebView2 Runtime version     | n/a                              | _(fill in)_                 |
| Branch / commit              | `spike/m0-harness` @ _(fill in)_ | same                        |

This spike depends on Task 0.3 (SP1). It reuses the SP1 harness's `WindowBuilder` main window
and `add_child` mechanism; see `docs/spikes/SP1.md` for that part's own verification status.

## Configuration

- Namespace UUID: a single fixed constant, `SPIKE_NAMESPACE` in
  `src-tauri/src/spike/data_store.rs`, generated once (not re-derived at runtime) and never
  changed across runs — changing it would change every derived UUID and orphan existing data
  stores. Its origin: an arbitrary UUIDv4 generated once for this spike and hard-coded (design
  §2.2.3 specifies "a fixed project namespace UUID (a constant in code)"; the real app will
  need its own project-wide constant chosen the same way, not necessarily this same value).
- Slot names / derived keys: `isolated:a` and `isolated:b`, following the `profile` key scheme
  in design §2.2.3 (`"isolated"` name → key `isolated:<service_id>`, here using the slot name
  directly as the id for the spike).
- UUID derivation: `Uuid::new_v5(&SPIKE_NAMESPACE, key.as_bytes())`. UUIDv4 is not used
  anywhere (design §10: never use a zero/placeholder value; the spike also avoids the
  non-deterministic v4 form since the whole point is a stable, reproducible identifier).
- Storage location:
  - **macOS:** `WebviewBuilder::data_store_identifier(uuid.into_bytes())` — a 16-byte array;
    WKWebView owns the on-disk location, not directly inspectable from application code.
  - **Windows:** `WebviewBuilder::data_directory(app_data_dir.join("webview").join(uuid_string))`
    — an explicit directory under the app's local data dir.
- Both children load `https://www.icloud.com/` initially; `incognito` is not set on either.

## Procedure

```sh
pnpm install
pnpm build
ELUMA_SPIKE=sp2 pnpm tauri dev
```

Windows PowerShell:

```powershell
pnpm install
pnpm build
$env:ELUMA_SPIKE = "sp2"; pnpm tauri dev
```

Before the first sign-in run, delete any pre-existing data for both slots (Windows: the
`{app_data_dir}/webview/<uuid>` directory printed at startup for each slot; macOS: the
corresponding WebKit data store — see "Constraints" below). If the data store cannot be
located or deleted on macOS, stop here and record the result as **Unconfirmed**; do not
proceed to the sign-in steps below with a pre-existing store still in place. Then:

1. Sign in to account A on the left child, account B on the right child, completing 2FA and
   choosing "trust this browser" for each. Reload both and confirm each shows the correct,
   distinct mailbox.
2. Quit the app normally and relaunch with the same mode. Confirm: the printed UUIDs are
   identical to the previous run, both accounts appear signed in without re-authentication,
   and the app does not crash.
3. Repeat step 2 but force-quit the app instead of quitting normally, as an additional
   (non-blocking) observation.

Do not commit real iCloud account details, and do not include them in screenshots.

## Results

Record each item as **Success**, **Failure**, or **Unconfirmed**. For a Failure, record the
symptom, exact reproduction steps, any error text, and a suspected cause. Do not fill in a
result you did not actually observe.

| Item                                                                                       | macOS          | Windows        |
| ------------------------------------------------------------------------------------------ | -------------- | -------------- |
| Startup logs both slots' UUIDs (and, on Windows, both `data_directory` paths)              | 未実施 (owner) | 未実施 (owner) |
| Two distinct accounts sign in side by side, each showing its own mailbox after reload      | 未実施 (owner) | 未実施 (owner) |
| Data store location identified (path/dir) — record "未確認" on macOS if it cannot be found | 未実施 (owner) | 未実施 (owner) |
| Normal restart: UUIDs unchanged, both accounts still signed in, no crash                   | 未実施 (owner) | 未実施 (owner) |
| Force-quit restart (additional observation): same checks                                   | 未実施 (owner) | 未実施 (owner) |

## Constraints

- macOS 14+ only; Linux and Android are out of scope for this spike and for Eluma generally.
- WKWebView owns the on-disk location of a `data_store_identifier` store; it is not directly
  inspectable from application code. If the owner cannot locate or delete the pre-existing
  store for a slot before the first sign-in run, stop and record the result as **Unconfirmed**
  rather than continuing the sign-in steps with an existing store still in place — a stale
  store would invalidate the "no re-authentication needed" check in step 2.
- The window created by this harness mode does not handle resizing (per the SP2 task scope);
  only SP1's mode exercises resize/relayout.
- A force-quit is a risk to flag for later design, not a pass/fail gate for this spike's
  completion criterion (which is the normal-restart case).

## Conclusion and impact on later tasks

_(Owner fills in after running the procedure.)_

- Recommended identifier-generation rule for the real implementation (namespace UUID source,
  key scheme) — confirm or amend design §2.2.3's scheme based on what was actually observed.
- Whether Task 1.5 (profile resolution and backends) can proceed as designed, or needs a
  fallback approach.
- Do not commit real iCloud account information to this repository.
