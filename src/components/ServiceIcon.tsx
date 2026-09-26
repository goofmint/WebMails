/**
 * One sidebar entry (Task 1.10; design.md §2.2.13). Renders the cached icon
 * PNG (Task 1.14; design.md §2.2.10, §11.1) through Tauri's asset protocol
 * when one exists for this service, falling back to a generated letter
 * icon — coloured deterministically from the service id — when there is no
 * cache yet, or the cached image fails to load. Also renders this
 * service's status `Badge` (Task 2.10), top-right, over the icon.
 */

import { useState, type DragEvent } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { CachedIconInfo, ServiceConfig, ServiceStatus } from "../ipc";
import { Badge } from "./Badge";
import { badgeLabel } from "./badgeLabel";
import { colorForServiceId, initialForService } from "../lib/color";

export interface ServiceIconProps {
  readonly service: ServiceConfig;
  /** From `Snapshot.icons[service.id]` — `undefined` when nothing is cached. */
  readonly cachedIcon: CachedIconInfo | undefined;
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
  cachedIcon,
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
  // Tracks the *source* (path + cache-busting version) that last failed to
  // load, not just whether some image ever failed: a stale failure must not
  // suppress a newer cached PNG (a later `service-icon-changed` bumps
  // `cachedIcon.version`, producing a different `src`), so only an exact
  // match with the current source counts as "still failed".
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const src = cachedIcon ? `${convertFileSrc(cachedIcon.path)}?v=${cachedIcon.version}` : undefined;
  const showImage = src !== undefined && src !== failedSrc;
  const initial = initialForService(service.name, service.id);
  const letterColor = colorForServiceId(service.id);
  // Same label Badge itself would render (or `null` if Badge renders
  // nothing) — so the accessible name only mentions status when the Badge
  // is actually visible, and never drifts from what it says.
  const statusLabel = status === undefined ? null : badgeLabel(status, badgesEnabled);
  const accessibleName = statusLabel === null ? service.name : `${service.name}, ${statusLabel}`;

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
      aria-label={accessibleName}
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
      {showImage && src !== undefined ? (
        <img
          className="service-icon__image"
          src={src}
          alt=""
          onError={() => {
            setFailedSrc(src);
          }}
        />
      ) : (
        <span className="service-icon__initial" style={{ backgroundColor: letterColor }}>
          {initial}
        </span>
      )}
      {status !== undefined && <Badge status={status} enabled={badgesEnabled} />}
    </button>
  );
}
