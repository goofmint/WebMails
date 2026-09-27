/**
 * Typed wrappers over the three shell-facing events (design.md §2.2.12,
 * §2.2.13; src-tauri/src/services/mod.rs's `emit_services_changed` /
 * `emit_select_service`). Each wrapper returns the `UnlistenFn` `listen`
 * itself resolves to, so a caller can `await` the subscription and later
 * call the result to unsubscribe.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ServiceStatus } from "./types";

/**
 * `services-changed` (`{ services: [...] }` on the wire — services/mod.rs's
 * `ServicesChangedPayload`). The wrapper deliberately does not decode that
 * payload: the store's job is to refetch `get_snapshot` on this event, so
 * the callback here is payload-less, a plain "something changed" signal.
 */
export async function onServicesChanged(callback: () => void): Promise<UnlistenFn> {
  return listen("services-changed", () => {
    callback();
  });
}

/**
 * `select-service` — wire payload `{ serviceId }`
 * (`SelectServicePayload`) — exposed to the caller as `{ id }`.
 */
export async function onSelectService(
  callback: (payload: { readonly id: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ readonly serviceId: string }>("select-service", (event) => {
    callback({ id: event.payload.serviceId });
  });
}

/**
 * `status-changed` (design.md §3.2: `{ kind, count?, reason? }` is the
 * `ServiceStatus` shape; the event itself pairs it with the service id).
 * Not emitted by any Rust code yet (Task 2.3), but consumed by the shell
 * store from Task 2.10 on: it merges each event into `snapshot.statuses`,
 * keyed by `serviceId`, so the sidebar's `Badge`s update live.
 */
export async function onStatusChanged(
  callback: (payload: { readonly serviceId: string; readonly status: ServiceStatus }) => void,
): Promise<UnlistenFn> {
  return listen<{ readonly serviceId: string; readonly status: ServiceStatus }>(
    "status-changed",
    (event) => {
      callback(event.payload);
    },
  );
}

/**
 * `service-icon-changed` — wire payload `{ serviceId }` (Task 1.14;
 * design.md §2.2.10), exposed to the caller as `{ id }` (matching
 * `onSelectService`'s own shape). Emitted after icon resolution writes a
 * new cached PNG for a service; the store's job is to refetch
 * `get_snapshot` on this event, same as `onServicesChanged`.
 */
export async function onServiceIconChanged(
  callback: (payload: { readonly id: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ readonly serviceId: string }>("service-icon-changed", (event) => {
    callback({ id: event.payload.serviceId });
  });
}
