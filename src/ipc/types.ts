/**
 * Frontend mirrors of the Rust DTOs `get_snapshot` and the shell events
 * carry (design.md §2.2.12, §3.2; src-tauri/src/commands/{dto,snapshot}.rs,
 * src-tauri/src/config/model.rs). Every field name and casing here is
 * chosen to match what actually crosses the IPC boundary as JSON, not the
 * Rust identifier — see each type's own comment for the serde rule that
 * produced it.
 */

/**
 * `[settings]` (src-tauri/src/config/model.rs `Settings`). All four fields
 * are plain `#[derive(Serialize)]` with no `rename`/`rename_all`, so they
 * stay snake_case on the wire — only the wrapping `Snapshot`'s own
 * top-level keys are camelCase (see below).
 */
export interface Settings {
  readonly reconcile_interval_seconds: number;
  readonly notifications: boolean;
  readonly notification_batch_threshold: number;
  readonly badge_sidebar: boolean;
}

/**
 * `[[services]].icon` (`IconSource` in config/model.rs), serialized with
 * `#[serde(tag = "source", content = "value", rename_all = "lowercase")]`:
 * an adjacently tagged enum, so the unit `Favicon` variant has no `value`
 * key at all, while `File`/`Url` carry their inner value under `value`.
 */
export type IconSource =
  | { readonly source: "favicon" }
  | { readonly source: "file"; readonly value: string }
  | { readonly source: "url"; readonly value: string };

/**
 * One `[[services]]` entry (`ServiceConfig`). `id`, `url` and `profile` are
 * `#[serde(transparent)]`/`Url` newtypes that serialize as plain strings.
 */
export interface ServiceConfig {
  readonly id: string;
  readonly name: string;
  readonly url: string;
  readonly profile: string;
  readonly notifications: boolean;
  readonly icon: IconSource;
}

/**
 * A `config.toml` load/parse/validation failure (`ConfigError`). `file` is
 * a `PathBuf`, which serde serializes as a plain string; `key` is `None`
 * only for failures that precede key resolution (I/O or TOML syntax
 * errors), so it round-trips as `null` rather than being omitted.
 */
export interface ConfigErrorInfo {
  readonly file: string;
  readonly key: string | null;
  readonly reason: string;
}

/**
 * A `statuses` map entry (design.md §3.2: `{ kind, count?, reason? }`).
 * `get_snapshot`'s `statuses` map is always empty until Task 2.3 gives it
 * a real value type (`commands/snapshot.rs`'s `BTreeMap<ServiceId, ()>`
 * comment), so this type only fixes the *shape* the wire will eventually
 * carry — it has no behaviour of its own in this task (badges are Task
 * 2.10).
 */
export interface ServiceStatus {
  readonly kind: "loading" | "ok" | "needsAttention" | "stale";
  readonly count?: number;
  readonly reason?: string;
}

/**
 * `get_snapshot`'s response (`SnapshotDto`). Only this wrapper's own keys
 * are camelCase (`#[serde(rename = "sidebarWidth")]`,
 * `#[serde(rename = "activeServiceId")]`,
 * `#[serde(rename = "configError", skip_serializing_if = "Option::is_none")]`)
 * — `settings` has no such attribute, so it always serializes, as `null`
 * when the app failed to start (`configError` is `Some` in that case).
 * `activeServiceId` has no `skip_serializing_if` either, so — like
 * `settings` — it always serializes, as `null` when there is no active
 * service (Task 1.10; `ServiceManager::Ready::active`, `None` in the
 * `Failed` state too).
 */
export interface Snapshot {
  readonly settings: Settings | null;
  readonly services: readonly ServiceConfig[];
  readonly statuses: Readonly<Record<string, ServiceStatus>>;
  readonly sidebarWidth: number;
  readonly activeServiceId: string | null;
  readonly configError?: ConfigErrorInfo;
}
