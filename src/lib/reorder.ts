/**
 * Pure HTML5-drag-and-drop reordering logic for the sidebar (Task 1.10).
 * Kept separate from any component so it is trivially unit-testable.
 */

/**
 * Returns `ids` with `draggedId` moved to sit where `targetId` currently
 * is. Dropping onto its own position (`draggedId === targetId`) returns
 * the original array reference unchanged, so callers can detect a no-op
 * drop with `===` instead of a deep comparison. Returns `ids` unchanged
 * (by reference) if either id is not present.
 */
export function moveId(
  ids: readonly string[],
  draggedId: string,
  targetId: string,
): readonly string[] {
  if (draggedId === targetId) {
    return ids;
  }

  const fromIndex = ids.indexOf(draggedId);
  const targetIndex = ids.indexOf(targetId);
  if (fromIndex === -1 || targetIndex === -1) {
    return ids;
  }

  // Removing at `fromIndex` first, then inserting at the *original*
  // `targetIndex` (not recomputed afterwards), makes the dragged item take
  // over the target's original absolute position — everything between the
  // two shifts by one to fill the gap, in either direction.
  const next = ids.slice();
  next.splice(fromIndex, 1);
  next.splice(targetIndex, 0, draggedId);
  return next;
}
