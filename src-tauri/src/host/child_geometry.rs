//! Pure geometry math for [`super::child_windows::ChildWindowHost`]
//! (design.md §2.2.4's fallback bullet, §8.1; Task 1.7).
//!
//! Unlike [`super::layout`], whose [`super::layout::Rect`] is a
//! window-relative rectangle in *logical* pixels (right for `add_child`,
//! which positions a child webview inside its parent window's own
//! coordinate space), [`ChildWindowHost`](super::child_windows::ChildWindowHost)'s
//! service windows are separate top-level OS windows. Positioning them
//! needs *physical*, desktop-absolute coordinates instead — the same
//! space [`tauri::Window::inner_position`]/[`tauri::Window::available_monitors`]
//! report in — so this module works in [`PhysicalRect`] throughout and
//! never touches [`super::layout::Rect`].
//!
//! As with [`super::layout`], nothing here depends on Tauri: every
//! function takes and returns plain `i32`/`u32`/`f64`/[`PhysicalRect`]
//! values, so [`super::child_windows`] is the only caller that turns a
//! live `Window`/`Monitor` into these inputs, and the coordinate math
//! itself is unit-tested without a running application.

use super::layout::SIDEBAR_WIDTH;

/// A rectangle in physical pixels: a desktop-absolute `(x, y)` origin plus
/// a `width` and `height`.
///
/// `x`/`y` may be negative — every monitor arrangement has *some* point of
/// reference, and [`offscreen_frame`] deliberately produces coordinates to
/// the left of every monitor, which are negative whenever the leftmost
/// monitor's own origin is at `x = 0` (the common case). `width`/`height`
/// match [`tauri::PhysicalSize`]'s fields, so they are never negative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Computes the frame for one service window (design.md §2.2.4's fallback
/// bullet): the parent's content area (right of the sidebar) when
/// `active`, or a frame the same size shoved fully left of every
/// monitor's bounding box when not.
///
/// `parent_position`/`parent_size`/`scale_factor` are the main window's
/// current physical inner (client-area) position, physical inner size and
/// scale factor — the same three values [`super::multiwebview`]'s
/// `current_content_rect` reads from a live `Window`, just not yet
/// converted to logical pixels. `monitors` is every monitor's physical
/// bounding rect (`Window::available_monitors`, converted).
///
/// Returns `None` when there is nowhere sensible to place the window: the
/// parent's physical size is zero, the parent is no wider than the
/// physical sidebar width (so the content area would have zero or
/// negative width), or (only when `active` is `false`, since only the
/// offscreen branch needs it) `monitors` is empty.
pub fn service_frame(
    parent_position: (i32, i32),
    parent_size: (u32, u32),
    scale_factor: f64,
    monitors: &[PhysicalRect],
    active: bool,
) -> Option<PhysicalRect> {
    let content = content_frame(parent_position, parent_size, scale_factor)?;
    if active {
        return Some(content);
    }
    let bbox = monitors_bounding_box(monitors)?;
    Some(offscreen_frame(
        bbox,
        (content.width, content.height),
        content.y,
    ))
}

/// The parent's content area in physical pixels: everything right of the
/// sidebar, same height as the parent's own physical content height.
///
/// `SIDEBAR_WIDTH` is logical; multiplying by `scale_factor` converts it
/// to the physical sidebar width this function subtracts. Every
/// float-to-int conversion below uses `as`, which — for `f64 -> i32`/`u32`
/// — saturates rather than panicking or wrapping (guaranteed since Rust
/// 1.45), so this never panics even for pathological inputs.
fn content_frame(
    parent_position: (i32, i32),
    parent_size: (u32, u32),
    scale_factor: f64,
) -> Option<PhysicalRect> {
    let (parent_x, parent_y) = parent_position;
    let (parent_width, parent_height) = parent_size;
    if parent_width == 0 || parent_height == 0 {
        return None;
    }

    let sidebar_physical = SIDEBAR_WIDTH * scale_factor;
    if !sidebar_physical.is_finite() || sidebar_physical < 0.0 {
        return None;
    }
    let sidebar_physical = sidebar_physical.round();

    if (parent_width as f64) <= sidebar_physical {
        return None;
    }

    let content_width = (parent_width as f64 - sidebar_physical) as u32;
    let x = (parent_x as f64 + sidebar_physical) as i32;

    Some(PhysicalRect {
        x,
        y: parent_y,
        width: content_width,
        height: parent_height,
    })
}

/// The smallest physical rectangle containing every monitor in `monitors`.
/// `None` for an empty slice — there is no bounding box of nothing.
///
/// Accumulates in `i64` so a monitor's `x + width` (both already valid
/// `i32`/`u32` values individually) cannot overflow while being summed,
/// then clamps back into `i32`/`u32` range at the end via [`i64::clamp`]
/// (never `as`, which would wrap rather than saturate for integer-to-
/// integer casts).
fn monitors_bounding_box(monitors: &[PhysicalRect]) -> Option<PhysicalRect> {
    let mut monitors = monitors.iter();
    let first = monitors.next()?;

    let mut min_x = i64::from(first.x);
    let mut min_y = i64::from(first.y);
    let mut max_x = min_x + i64::from(first.width);
    let mut max_y = min_y + i64::from(first.height);

    for monitor in monitors {
        let x = i64::from(monitor.x);
        let y = i64::from(monitor.y);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + i64::from(monitor.width));
        max_y = max_y.max(y + i64::from(monitor.height));
    }

    Some(PhysicalRect {
        x: min_x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: min_y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        width: (max_x - min_x).clamp(0, i64::from(u32::MAX)) as u32,
        height: (max_y - min_y).clamp(0, i64::from(u32::MAX)) as u32,
    })
}

/// A frame of `size` placed so it touches, and lies entirely left of, the
/// left edge of `monitors_bbox` (`x + width == monitors_bbox.x`) — fully
/// outside every monitor's bounds regardless of which monitor the parent
/// window is on, mirroring why [`super::layout::offscreen_rect`] shoves
/// inactive services past the *window's* left edge rather than hiding
/// them (design.md §2.2.4, §8.2; tauri#11376).
fn offscreen_frame(monitors_bbox: PhysicalRect, size: (u32, u32), y: i32) -> PhysicalRect {
    let (width, height) = size;
    let x = i64::from(monitors_bbox.x) - i64::from(width);
    PhysicalRect {
        x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect {
        PhysicalRect {
            x,
            y,
            width,
            height,
        }
    }

    // --- scale factors ---------------------------------------------------

    #[test]
    fn active_frame_at_scale_1_0() {
        let frame = service_frame((100, 50), (1000, 700), 1.0, &[], true);
        assert_eq!(
            frame,
            Some(PhysicalRect {
                x: 164,
                y: 50,
                width: 936,
                height: 700,
            })
        );
    }

    #[test]
    fn active_frame_at_scale_2_0() {
        // Physically double the logical fixture above: parent size and
        // sidebar width both scale, so the *logical* layout is identical.
        let frame = service_frame((200, 100), (2000, 1400), 2.0, &[], true);
        assert_eq!(
            frame,
            Some(PhysicalRect {
                x: 328, // 200 + 64*2
                y: 100,
                width: 1872, // 2000 - 128
                height: 1400,
            })
        );
    }

    // --- negative parent coordinates -------------------------------------

    #[test]
    fn active_frame_with_negative_parent_position() {
        // A window dragged partly onto a monitor to the left of the
        // primary (negative-origin) monitor.
        let frame = service_frame((-500, -300), (1000, 700), 1.0, &[], true);
        assert_eq!(
            frame,
            Some(PhysicalRect {
                x: -436, // -500 + 64
                y: -300,
                width: 936,
                height: 700,
            })
        );
    }

    // --- zero / degenerate sizes ------------------------------------------

    #[test]
    fn zero_width_is_none() {
        assert_eq!(service_frame((0, 0), (0, 700), 1.0, &[], true), None);
    }

    #[test]
    fn zero_height_is_none() {
        assert_eq!(service_frame((0, 0), (1000, 0), 1.0, &[], true), None);
    }

    #[test]
    fn parent_no_wider_than_sidebar_is_none() {
        // Exactly the physical sidebar width: zero content width.
        assert_eq!(service_frame((0, 0), (64, 700), 1.0, &[], true), None);
        // Narrower still.
        assert_eq!(service_frame((0, 0), (10, 700), 1.0, &[], true), None);
    }

    #[test]
    fn parent_no_wider_than_sidebar_is_none_when_inactive_too() {
        let monitors = [monitor(0, 0, 1920, 1080)];
        assert_eq!(
            service_frame((0, 0), (64, 700), 1.0, &monitors, false),
            None
        );
    }

    // --- multi-monitor bounding box, offscreen placement -------------------

    #[test]
    fn offscreen_frame_sits_left_of_single_monitor() {
        let monitors = [monitor(0, 0, 1920, 1080)];
        let frame = service_frame((0, 0), (1000, 700), 1.0, &monitors, false);
        assert_eq!(
            frame,
            Some(PhysicalRect {
                x: -936, // 0 - content width (936)
                y: 0,
                width: 936,
                height: 700,
            })
        );
    }

    #[test]
    fn offscreen_frame_sits_left_of_the_bounding_box_of_several_monitors() {
        // Primary at (0,0) 1920x1080, a secondary to its right, taller and
        // starting lower (e.g. a portrait-rotated monitor).
        let monitors = [monitor(0, 0, 1920, 1080), monitor(1920, -200, 1080, 1920)];
        let frame = service_frame((0, 0), (1000, 700), 1.0, &monitors, false);
        // Bounding box left edge is still x = 0 (the leftmost monitor's
        // origin), regardless of the second monitor's extent.
        assert_eq!(
            frame,
            Some(PhysicalRect {
                x: -936,
                y: 0,
                width: 936,
                height: 700,
            })
        );
    }

    #[test]
    fn offscreen_frame_accounts_for_a_monitor_left_of_the_origin() {
        // A monitor placed left of (0,0) — the common "extend desktop to
        // the left" arrangement — must pull the bounding box's left edge
        // (and so the offscreen x) further left than 0.
        let monitors = [monitor(0, 0, 1920, 1080), monitor(-1920, 0, 1920, 1080)];
        let frame = service_frame((0, 0), (1000, 700), 1.0, &monitors, false);
        assert_eq!(
            frame,
            Some(PhysicalRect {
                x: -1920 - 936,
                y: 0,
                width: 936,
                height: 700,
            })
        );
    }

    #[test]
    fn offscreen_frame_never_overlaps_any_monitor() {
        let monitor_sets: [&[PhysicalRect]; 3] = [
            &[monitor(0, 0, 1920, 1080)],
            &[monitor(0, 0, 1920, 1080), monitor(1920, 0, 1080, 1920)],
            &[monitor(-1920, 0, 1920, 1080), monitor(0, 0, 2560, 1440)],
        ];
        for monitors in monitor_sets {
            let bbox = monitors_bounding_box(monitors).expect("non-empty monitor set");
            let frame = service_frame((0, 0), (1000, 700), 1.0, monitors, false)
                .expect("valid parent geometry");
            assert!(
                frame.x + frame.width as i32 <= bbox.x,
                "offscreen frame {frame:?} overlaps monitor bounding box {bbox:?}"
            );
        }
    }

    #[test]
    fn empty_monitors_is_none_when_inactive() {
        assert_eq!(service_frame((0, 0), (1000, 700), 1.0, &[], false), None);
    }

    #[test]
    fn active_frame_ignores_monitors() {
        // The active branch never needs monitor info, even when there is
        // none — mirrors the empty-slice case above returning `Some` here
        // rather than `None`.
        assert_eq!(
            service_frame((0, 0), (1000, 700), 1.0, &[], true),
            service_frame((0, 0), (1000, 700), 1.0, &[monitor(0, 0, 1920, 1080)], true)
        );
    }
}
