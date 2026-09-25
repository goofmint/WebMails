//! Pure layout math for the shell sidebar and the service content area
//! (design.md §2.2.4).
//!
//! Nothing here depends on Tauri: every function takes and returns plain
//! `f64`s or [`Rect`] values, so the coordinate contract — where the
//! sidebar sits, where the active service goes, where inactive services
//! are shoved offscreen — is unit-tested without a running application.
//! [`super::multiwebview`] is the only caller that turns a [`Rect`] into a
//! Tauri position/size.

/// Width of the shell sidebar, in logical pixels. The single Rust constant
/// for `SIDEBAR_WIDTH` (design.md §2.2.4: "`SIDEBAR_WIDTH` is a single
/// Rust constant (64px)"); every other module that needs the sidebar
/// width reads this one instead of repeating the number.
pub const SIDEBAR_WIDTH: f64 = 64.0;

/// A rectangle in logical pixels: an `(x, y)` origin plus a `width` and
/// `height`.
///
/// `x`/`y` may be negative — [`offscreen_rect`] always produces a negative
/// `x` — but `width`/`height` are never negative: every function below
/// that derives a size saturates it to `0.0` (see each function's doc
/// comment for why that saturation is this module's own policy, not
/// something design.md requires).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}

/// Converts a physical window size and scale factor to a logical size
/// (`physical / scale_factor`), the arithmetic behind Tauri's own
/// `PhysicalSize::to_logical`. Every physical→logical conversion in this
/// crate goes through this one function, for the same "single source of
/// truth" reason [`SIDEBAR_WIDTH`] is a single constant.
///
/// Saturates the result to `0.0` or greater. This is this module's own
/// policy, not a contract design.md states: a negative or non-finite
/// physical size should never occur in practice (Tauri's `PhysicalSize`
/// fields are unsigned), but saturating here means every caller downstream
/// can treat width/height as non-negative without re-checking.
pub fn physical_to_logical(width: f64, height: f64, scale_factor: f64) -> (f64, f64) {
    (
        (width / scale_factor).max(0.0),
        (height / scale_factor).max(0.0),
    )
}

/// The shell sidebar's rect: pinned to the window's top-left corner, fixed
/// width [`SIDEBAR_WIDTH`], full window height (design.md §2.2.4: "The
/// shell is `add_child` at `(0, 0, SIDEBAR_WIDTH, height)`").
///
/// Saturates `window_height` to `0.0` or greater — this module's own
/// policy (see [`physical_to_logical`]), not a design.md contract.
pub fn shell_rect(window_height: f64) -> Rect {
    Rect::new(0.0, 0.0, SIDEBAR_WIDTH, window_height.max(0.0))
}

/// The content area to the right of the sidebar. This is where the
/// *active* service webview is positioned (design.md §2.2.4).
///
/// Saturates both dimensions to `0.0` or greater — including when
/// `window_width` is less than [`SIDEBAR_WIDTH`], which would otherwise
/// make the content width negative. This is this module's own policy (see
/// [`physical_to_logical`]), not a design.md contract: design.md does not
/// say what happens when the window is narrower than the sidebar.
pub fn content_rect(window_width: f64, window_height: f64) -> Rect {
    Rect::new(
        SIDEBAR_WIDTH,
        0.0,
        (window_width - SIDEBAR_WIDTH).max(0.0),
        window_height.max(0.0),
    )
}

/// Where the *active* service webview is positioned: exactly the content
/// rect (design.md §2.2.4: "The active service is placed at `x =
/// SIDEBAR_WIDTH`"). A separate function from [`content_rect`], distinct
/// in name only, so call sites at [`super::multiwebview`] name the concept
/// they mean (`activate`/`relayout` ask "where does the active service
/// go?", not "what's the content rect?").
pub fn active_rect(content: Rect) -> Rect {
    content
}

/// Where an *inactive* service webview is positioned: shoved fully past
/// the window's left edge, `content.width + SIDEBAR_WIDTH` logical pixels
/// to the left of `x = 0` (design.md §2.2.4: "Inactive ones are placed at
/// `x = -(content width + SIDEBAR_WIDTH)`"). Same size as the content
/// rect, so a service keeps its content-rect dimensions whether active or
/// not.
///
/// Inactive services are placed here, never hidden or `set_visible(false)`
/// — design.md §2.2.4 and §8.2 are explicit that this is deliberate, so
/// each service's timers/observers keep running while offscreen, and so
/// the host does not depend on z-order between child webviews
/// (tauri#11376).
pub fn offscreen_rect(content: Rect) -> Rect {
    Rect::new(
        -(content.width + SIDEBAR_WIDTH),
        content.y,
        content.width,
        content.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- standard size ----------------------------------------------------

    #[test]
    fn shell_rect_is_pinned_top_left_full_height() {
        assert_eq!(shell_rect(700.0), Rect::new(0.0, 0.0, SIDEBAR_WIDTH, 700.0));
    }

    #[test]
    fn content_rect_starts_right_of_the_sidebar() {
        assert_eq!(
            content_rect(1000.0, 700.0),
            Rect::new(64.0, 0.0, 936.0, 700.0)
        );
    }

    #[test]
    fn active_rect_is_exactly_the_content_rect() {
        let content = content_rect(1000.0, 700.0);
        assert_eq!(active_rect(content), content);
    }

    #[test]
    fn offscreen_rect_is_content_sized_and_shoved_left() {
        let content = content_rect(1000.0, 700.0);
        assert_eq!(
            offscreen_rect(content),
            Rect::new(-1000.0, 0.0, 936.0, 700.0)
        );
    }

    #[test]
    fn offscreen_rect_never_overlaps_the_window() {
        // The window spans x = 0..window_width; an inactive service's
        // rect must lie entirely to the left of x = 0 for every window
        // size, not just the one fixture size above.
        for window_width in [0.0, 10.0, 64.0, 200.0, 1000.0, 4000.0] {
            let content = content_rect(window_width, 700.0);
            let offscreen = offscreen_rect(content);
            assert!(
                offscreen.x + offscreen.width <= 0.0,
                "offscreen rect {offscreen:?} overlaps the window for width {window_width}"
            );
        }
    }

    // --- scale factors ------------------------------------------------------

    #[test]
    fn physical_to_logical_at_scale_1_0() {
        assert_eq!(physical_to_logical(1000.0, 700.0, 1.0), (1000.0, 700.0));
    }

    #[test]
    fn physical_to_logical_at_scale_1_5() {
        assert_eq!(physical_to_logical(1500.0, 1050.0, 1.5), (1000.0, 700.0));
    }

    #[test]
    fn physical_to_logical_at_scale_2_0() {
        assert_eq!(physical_to_logical(2000.0, 1400.0, 2.0), (1000.0, 700.0));
    }

    #[test]
    fn rects_are_identical_across_equivalent_scale_factors() {
        let sizes = [
            (1000.0, 700.0, 1.0),
            (1500.0, 1050.0, 1.5),
            (2000.0, 1400.0, 2.0),
        ];
        let expected_content = content_rect(1000.0, 700.0);
        let expected_offscreen = offscreen_rect(expected_content);
        for (w, h, scale) in sizes {
            let (logical_w, logical_h) = physical_to_logical(w, h, scale);
            let content = content_rect(logical_w, logical_h);
            assert_eq!(content, expected_content, "scale {scale}");
            assert_eq!(offscreen_rect(content), expected_offscreen, "scale {scale}");
        }
    }

    // --- zero sizes -----------------------------------------------------

    #[test]
    fn physical_to_logical_of_zero_is_zero() {
        assert_eq!(physical_to_logical(0.0, 0.0, 2.0), (0.0, 0.0));
    }

    #[test]
    fn shell_rect_at_zero_height() {
        assert_eq!(shell_rect(0.0), Rect::new(0.0, 0.0, SIDEBAR_WIDTH, 0.0));
    }

    #[test]
    fn content_rect_at_zero_size() {
        assert_eq!(
            content_rect(0.0, 0.0),
            Rect::new(SIDEBAR_WIDTH, 0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn offscreen_rect_at_zero_content_size() {
        let content = content_rect(0.0, 0.0);
        assert_eq!(
            offscreen_rect(content),
            Rect::new(-SIDEBAR_WIDTH, 0.0, 0.0, 0.0)
        );
    }

    // --- width narrower than the sidebar ---------------------------------

    #[test]
    fn content_rect_saturates_when_window_is_narrower_than_the_sidebar() {
        // window_width (50) < SIDEBAR_WIDTH (64): content width would be
        // negative without saturation.
        assert_eq!(
            content_rect(50.0, 700.0),
            Rect::new(SIDEBAR_WIDTH, 0.0, 0.0, 700.0)
        );
    }

    #[test]
    fn offscreen_rect_saturates_when_window_is_narrower_than_the_sidebar() {
        let content = content_rect(50.0, 700.0);
        assert_eq!(
            offscreen_rect(content),
            Rect::new(-SIDEBAR_WIDTH, 0.0, 0.0, 700.0)
        );
    }
}
