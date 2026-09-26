/**
 * The settings screen's service list (Task 1.12; design.md §2.2.13:
 * "Service list with edit and delete."). Tracks which single service (if
 * any) is being edited or deleted, but derives the *effective* id from
 * whether that service is still present in `services` on every render
 * (rather than a `useEffect` that would need to call `setState` in
 * response to a prop change, which is itself just derived state) — so the
 * form for a service closes as soon as it disappears from the list (e.g.
 * removed from another settings-window instance, or by this window's own
 * successful delete).
 */

import { useState } from "react";
import type { ServiceConfig, SettingsIpc } from "../ipc";
import { distinctNamedProfiles } from "./helpers";
import { EditServiceForm } from "./EditServiceForm";
import { DeleteConfirm } from "./DeleteConfirm";

export interface ServiceListProps {
  readonly ipc: SettingsIpc;
  readonly services: readonly ServiceConfig[];
}

export function ServiceList({ ipc, services }: ServiceListProps) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const effectiveEditingId =
    editingId !== null && services.some((service) => service.id === editingId) ? editingId : null;
  const effectiveDeletingId =
    deletingId !== null && services.some((service) => service.id === deletingId)
      ? deletingId
      : null;

  if (services.length === 0) {
    return <p className="service-list__empty">No services configured.</p>;
  }

  const existingNamedProfiles = distinctNamedProfiles(services);

  return (
    <ul className="service-list">
      {services.map((service) => (
        <li key={service.id} className="service-list__item">
          <div className="service-list__summary">
            <span className="service-list__name">{service.name}</span>
            <span className="service-list__url">{service.url}</span>
            <button
              type="button"
              onClick={() => {
                setEditingId(service.id);
                setDeletingId(null);
              }}
            >
              Edit
            </button>
            <button
              type="button"
              onClick={() => {
                setDeletingId(service.id);
                setEditingId(null);
              }}
            >
              Delete
            </button>
          </div>
          {effectiveEditingId === service.id && (
            <EditServiceForm
              ipc={ipc}
              service={service}
              existingNamedProfiles={existingNamedProfiles}
              onClose={() => {
                setEditingId(null);
              }}
            />
          )}
          {effectiveDeletingId === service.id && (
            <DeleteConfirm
              ipc={ipc}
              service={service}
              onClose={() => {
                setDeletingId(null);
              }}
            />
          )}
        </li>
      ))}
    </ul>
  );
}
