/**
 * The `shell` webview's whole UI (Task 1.10; design.md §2.2.13): a
 * vertical list of `ServiceIcon`s, HTML5 drag-and-drop reordering, "+" and
 * gear buttons, and a `ConfigErrorScreen` swapped in when the store
 * reports a `configError`.
 */

import { useRef, useState } from "react";
import { useShellState, useShellStore } from "../store/useShellStore";
import { ServiceIcon } from "./ServiceIcon";
import { ConfigErrorScreen } from "./ConfigErrorScreen";
import { moveId } from "../lib/reorder";
import "./sidebar.css";

export function Sidebar() {
  const state = useShellState();
  const store = useShellStore();
  const draggedIdRef = useRef<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);

  if (state.status === "loading") {
    return <aside className="sidebar" aria-label="Services" />;
  }

  if (state.status === "error") {
    return (
      <aside className="sidebar" aria-label="Services">
        <p className="sidebar__error">{state.message}</p>
      </aside>
    );
  }

  const { snapshot, selectedId } = state;
  const { configError } = snapshot;
  // `settings` is only ever `null` alongside a `configError` (design.md
  // §2.2.12's comment on `Snapshot`), and that case already returned above —
  // so by this point it is always present.
  const badgesEnabled = snapshot.settings !== null && snapshot.settings.badge_sidebar;

  if (configError !== undefined) {
    return (
      <ConfigErrorScreen
        configError={configError}
        sidebarWidth={snapshot.sidebarWidth}
        onOpenSettings={() => {
          store.openSettings();
        }}
      />
    );
  }

  function handleDragStart(id: string): void {
    draggedIdRef.current = id;
  }

  function handleDragOverTarget(id: string): void {
    setDropTargetId(id);
  }

  function handleDrop(targetId: string): void {
    const draggedId = draggedIdRef.current;
    if (draggedId !== null) {
      const currentIds = snapshot.services.map((service) => service.id);
      const nextIds = moveId(currentIds, draggedId, targetId);
      if (nextIds !== currentIds) {
        store.reorder(nextIds);
      }
    }
    draggedIdRef.current = null;
    setDropTargetId(null);
  }

  function handleDragEnd(): void {
    draggedIdRef.current = null;
    setDropTargetId(null);
  }

  return (
    <aside className="sidebar" aria-label="Services">
      <ul className="sidebar__list">
        {snapshot.services.map((service) => (
          <li key={service.id}>
            <ServiceIcon
              service={service}
              cachedIcon={snapshot.icons[service.id]}
              selected={service.id === selectedId}
              size={snapshot.sidebarWidth}
              draggable
              isDropTarget={dropTargetId === service.id}
              status={snapshot.statuses[service.id]}
              badgesEnabled={badgesEnabled}
              onSelect={(id) => {
                store.select(id);
              }}
              onDragStart={handleDragStart}
              onDragOverTarget={handleDragOverTarget}
              onDrop={handleDrop}
              onDragEnd={handleDragEnd}
            />
          </li>
        ))}
      </ul>
      <div className="sidebar__actions">
        <button
          type="button"
          className="sidebar__add-button"
          aria-label="Add service"
          onClick={() => {
            store.openSettings();
          }}
        >
          +
        </button>
        <button
          type="button"
          className="sidebar__settings-button"
          aria-label="Settings"
          onClick={() => {
            store.openSettings();
          }}
        >
          ⚙
        </button>
      </div>
    </aside>
  );
}
