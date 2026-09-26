/**
 * One sidebar entry (Task 1.10; design.md §2.2.13). Renders a letter
 * placeholder for `favicon`/`file` icon sources (real icon resolution is
 * Task 1.14) and an `<img>` for a `url` source, falling back to the letter
 * placeholder if that image fails to load. Also renders this service's
 * status `Badge` (Task 2.10), top-right, over the icon.
 */

import { useState, type DragEvent } from "react";
import type { ServiceConfig, ServiceStatus } from "../ipc";
import { Badge } from "./Badge";

export interface ServiceIconProps {
  readonly service: ServiceConfig;
  readonly selected: boolean;
  /** Sidebar width in px, from `Snapshot.sidebarWidth` — never hard-coded. */
  readonly size: number;
  readonly draggable: boolean;
  readonly isDropTarget: boolean;
  /** `undefined` while the service has no webview yet (still waiting for
   * its staggered start), so it has no status and shows no badge. */
  readonly status: ServiceStatus | undefined;
  /** `settings.badge_sidebar` — forwarded to `Badge` as-is. */
  readonly badgesEnabled: boolean;
  readonly onSelect: (id: string) => void;
  readonly onDragStart: (id: string) => void;
  readonly onDragOverTarget: (id: string) => void;
  readonly onDrop: (id: string) => void;
  readonly onDragEnd: () => void;
}

export function ServiceIcon({
  service,
  selected,
  size,
  draggable,
  isDropTarget,
  status,
  badgesEnabled,
  onSelect,
  onDragStart,
  onDragOverTarget,
  onDrop,
  onDragEnd,
}: ServiceIconProps) {
  const [imageFailed, setImageFailed] = useState(false);
  const showImage = service.icon.source === "url" && !imageFailed;
  const initial = service.name.charAt(0).toUpperCase();

  const classNames = [
    "service-icon",
    selected && "service-icon--selected",
    isDropTarget && "service-icon--drop-target",
  ]
    .filter((name): name is string => Boolean(name))
    .join(" ");

  function handleDragStart(event: DragEvent<HTMLButtonElement>): void {
    event.dataTransfer?.setData("text/plain", service.id);
    onDragStart(service.id);
  }

  function handleDragOver(event: DragEvent<HTMLButtonElement>): void {
    event.preventDefault();
    onDragOverTarget(service.id);
  }

  function handleDrop(event: DragEvent<HTMLButtonElement>): void {
    event.preventDefault();
    onDrop(service.id);
  }

  return (
    <button
      type="button"
      className={classNames}
      style={{ width: size, height: size }}
      title={service.name}
      aria-label={service.name}
      aria-current={selected ? "true" : undefined}
      draggable={draggable}
      onClick={() => {
        onSelect(service.id);
      }}
      onDragStart={handleDragStart}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
      onDragEnd={onDragEnd}
    >
      {showImage ? (
        <img
          className="service-icon__image"
          src={service.icon.source === "url" ? service.icon.value : ""}
          alt=""
          onError={() => {
            setImageFailed(true);
          }}
        />
      ) : (
        <span className="service-icon__initial">{initial}</span>
      )}
      {status !== undefined && <Badge status={status} enabled={badgesEnabled} />}
    </button>
  );
}
