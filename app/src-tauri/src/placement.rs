//! Where the bar and its window go on screen.
//!
//! The bar can sit anywhere. The context menu and the usage panel do not move
//! it: the window grows toward the side of the bar that has room and the extra
//! content is drawn there. Everything here is pure (plain rectangles in physical
//! pixels, no window handles) so the rules are unit tested; `lib.rs` feeds it the
//! monitors' work areas and applies the result.

use serde::{Deserialize, Serialize};

/// A rectangle in physical screen pixels; the origin may be negative on a
/// multi-monitor desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }
}

/// The side of the bar the menu or panel is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Result of `place`: the window to apply and where the bar sits inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// The bar itself in screen coordinates, after anchoring and clamping.
    pub bar: Rect,
    /// The window that holds the bar plus the extra content.
    pub window: Rect,
    /// Where the extra content is drawn relative to the bar.
    pub side: Side,
    /// The bar's top-left relative to the window's top-left.
    pub bar_offset: Point,
}

/// Top-left of a window of `size` snapped to the right edge of a monitor,
/// vertically centered, `margin` physical pixels in from the edge. Pure so the
/// snap rule is testable; `monitor` is `(x, y, width, height)`. This is only the
/// first-launch (or off-screen) placement now, not something a drag re-applies.
pub fn right_center_origin(monitor: (i32, i32, u32, u32), size: (u32, u32), margin: i32) -> (i32, i32) {
    let (mx, my, mw, mh) = monitor;
    let x = mx + (mw as i32 - size.0 as i32) - margin;
    let y = my + (mh as i32 - size.1 as i32) / 2;
    (x, y)
}

/// Moves `rect` the least it takes to lie inside `area`. A rect larger than the
/// area on an axis is aligned to the area's top-left on that axis.
pub fn clamp_into(rect: Rect, area: Rect) -> Rect {
    let x = clamp_axis(rect.x, rect.width, area.x, area.width);
    let y = clamp_axis(rect.y, rect.height, area.y, area.height);
    Rect { x, y, ..rect }
}

/// One axis of `clamp_into`: `start`/`len` inside `area_start`/`area_len`.
fn clamp_axis(start: i32, len: u32, area_start: i32, area_len: u32) -> i32 {
    let max_start = (area_start as i64 + area_len as i64 - len as i64).max(area_start as i64);
    (start as i64).clamp(area_start as i64, max_start) as i32
}

/// Index of the area `rect` overlaps the most, or `None` when it overlaps none
/// (it is fully off every monitor).
pub fn pick_area(rect: Rect, areas: &[Rect]) -> Option<usize> {
    let mut best: Option<(usize, i64)> = None;
    for (i, area) in areas.iter().enumerate() {
        let w = rect.right().min(area.right()) as i64 - rect.x.max(area.x) as i64;
        let h = rect.bottom().min(area.bottom()) as i64 - rect.y.max(area.y) as i64;
        if w <= 0 || h <= 0 {
            continue;
        }
        if best.is_none_or(|(_, overlap)| w * h > overlap) {
            best = Some((i, w * h));
        }
    }
    best.map(|(i, _)| i)
}

/// The bar rectangle to start with: the saved top-left when it still touches a
/// monitor (clamped into it), otherwise right-center of `fallback` with `margin`.
pub fn restore_bar(
    saved: Option<(i32, i32)>,
    size: (u32, u32),
    areas: &[Rect],
    fallback: Rect,
    margin: i32,
) -> Rect {
    if let Some((x, y)) = saved {
        let wanted = Rect::new(x, y, size.0, size.1);
        if let Some(i) = pick_area(wanted, areas) {
            return clamp_into(wanted, areas[i]);
        }
    }
    let (x, y) = right_center_origin((fallback.x, fallback.y, fallback.width, fallback.height), size, margin);
    Rect::new(x, y, size.0, size.1)
}

/// Resizes the bar to `size` (collapse/expand) without throwing it elsewhere:
/// the vertical center stays, and so does the horizontal edge nearest the
/// monitor edge (the right edge in the right half of `area`, the left edge in
/// the left half). The result is clamped into `area`; an unchanged size only
/// clamps.
pub fn anchor_resize(bar: Rect, size: (u32, u32), area: Rect) -> Rect {
    // Doubled centers keep the half-pixel exact without floating point.
    let bar_mid = 2 * bar.x as i64 + bar.width as i64;
    let area_mid = 2 * area.x as i64 + area.width as i64;
    let x = if bar_mid >= area_mid { bar.right() - size.0 as i32 } else { bar.x };
    let y = bar.y + (bar.height / 2) as i32 - (size.1 / 2) as i32;
    clamp_into(Rect::new(x, y, size.0, size.1), area)
}

/// Window for a bar of `size` plus `extra` = (width, minimum height) of menu or
/// panel content, all inside `area`. The bar keeps its place on screen; the
/// window grows left when `extra.0` fits there, else right, else toward the side
/// with more room with a narrower window. Height grows around the bar's center
/// and is clamped into `area`.
pub fn place(bar: Rect, size: (u32, u32), extra: (u32, u32), area: Rect) -> Placement {
    let bar = anchor_resize(bar, size, area);

    let left_room = (bar.x as i64 - area.x as i64).max(0) as u32;
    let right_room = (area.right() as i64 - bar.right() as i64).max(0) as u32;
    let (side, extra_width) = if extra.0 == 0 || left_room >= extra.0 {
        (Side::Left, extra.0)
    } else if right_room >= extra.0 {
        (Side::Right, extra.0)
    } else if left_room >= right_room {
        (Side::Left, left_room)
    } else {
        (Side::Right, right_room)
    };

    let width = bar.width + extra_width;
    let height = bar.height.max(extra.1).min(area.height);
    let x = match side {
        Side::Left => bar.x - extra_width as i32,
        Side::Right => bar.x,
    };
    let centered_y = bar.y + (bar.height / 2) as i32 - (height / 2) as i32;
    let window = clamp_into(Rect::new(x, centered_y, width, height), area);

    Placement {
        bar,
        window,
        side,
        bar_offset: Point { x: bar.x - window.x, y: bar.y - window.y },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1920x1080 monitor whose work area loses 40px to the taskbar.
    const AREA: Rect = Rect { x: 0, y: 0, width: 1920, height: 1040 };
    const BAR: (u32, u32) = (72, 400);
    const TAB: (u32, u32) = (46, 46);

    fn assert_inside(inner: Rect, area: Rect) {
        assert!(
            inner.x >= area.x && inner.y >= area.y && inner.right() <= area.right() && inner.bottom() <= area.bottom(),
            "{inner:?} is not inside {area:?}"
        );
    }

    /// The bar's screen position is `window + offset`; it must equal `bar`.
    fn assert_bar_fixed(p: Placement) {
        assert_eq!(p.window.x + p.bar_offset.x, p.bar.x, "{p:?}");
        assert_eq!(p.window.y + p.bar_offset.y, p.bar.y, "{p:?}");
    }

    // ---- first-launch placement ----

    /// The bar hugs the right edge (margin in) and is centered vertically.
    #[test]
    fn snaps_to_the_right_edge_centered_vertically() {
        assert_eq!(right_center_origin((0, 0, 1920, 1080), (72, 400), 12), (1836, 340));
    }

    /// A secondary monitor's origin (here left of and above the primary, so
    /// negative) is honored on both axes.
    #[test]
    fn honors_the_monitor_origin() {
        assert_eq!(right_center_origin((-1920, -200, 1920, 1080), (72, 400), 12), (-84, 140));
    }

    // ---- clamp_into / pick_area ----

    #[test]
    fn clamp_moves_a_rect_back_inside_on_every_side() {
        let r = |x, y| Rect::new(x, y, 72, 400);
        assert_eq!(clamp_into(r(1900, 300), AREA), r(1848, 300));
        assert_eq!(clamp_into(r(-30, 300), AREA), r(0, 300));
        assert_eq!(clamp_into(r(500, -50), AREA), r(500, 0));
        assert_eq!(clamp_into(r(500, 900), AREA), r(500, 640));
        assert_eq!(clamp_into(r(500, 300), AREA), r(500, 300), "already inside: untouched");
    }

    #[test]
    fn clamp_aligns_a_rect_larger_than_the_area_to_its_origin() {
        let big = Rect::new(-500, 700, 3000, 2000);
        assert_eq!(clamp_into(big, AREA), Rect::new(0, 0, 3000, 2000));
    }

    #[test]
    fn pick_area_takes_the_largest_overlap_and_none_when_fully_off() {
        let left = Rect::new(-1920, 0, 1920, 1040);
        let areas = [AREA, left];
        assert_eq!(pick_area(Rect::new(100, 100, 72, 400), &areas), Some(0));
        assert_eq!(pick_area(Rect::new(-1000, 100, 72, 400), &areas), Some(1));
        // 60px of the bar on the left monitor, 12px on the primary.
        assert_eq!(pick_area(Rect::new(-60, 100, 72, 400), &areas), Some(1));
        assert_eq!(pick_area(Rect::new(5000, 100, 72, 400), &areas), None);
        assert_eq!(pick_area(Rect::new(0, 4000, 72, 400), &areas), None);
        // Touching an edge is not overlapping.
        assert_eq!(pick_area(Rect::new(1920, 100, 72, 400), &[AREA]), None);
    }

    // ---- restore ----

    #[test]
    fn a_valid_saved_position_is_restored_as_is() {
        let r = restore_bar(Some((900, 300)), BAR, &[AREA], AREA, 8);
        assert_eq!(r, Rect::new(900, 300, 72, 400));
    }

    #[test]
    fn a_partly_off_screen_position_is_clamped_in() {
        let r = restore_bar(Some((1900, 900)), BAR, &[AREA], AREA, 8);
        assert_eq!(r, Rect::new(1848, 640, 72, 400));
    }

    #[test]
    fn a_fully_off_screen_or_absent_position_falls_back_to_right_center() {
        // 1920 - 72 - 8 = 1840; (1040 - 400) / 2 = 320.
        let fallback = Rect::new(1840, 320, 72, 400);
        assert_eq!(restore_bar(Some((5000, 300)), BAR, &[AREA], AREA, 8), fallback);
        assert_eq!(restore_bar(Some((300, -9000)), BAR, &[AREA], AREA, 8), fallback);
        assert_eq!(restore_bar(None, BAR, &[AREA], AREA, 8), fallback);
    }

    #[test]
    fn restore_understands_a_monitor_at_negative_coordinates() {
        let left = Rect::new(-1920, -200, 1920, 1040);
        let areas = [AREA, left];
        assert_eq!(
            restore_bar(Some((-1500, -100)), BAR, &areas, AREA, 8),
            Rect::new(-1500, -100, 72, 400)
        );
        // Off every monitor: the fallback monitor decides, here the left one.
        assert_eq!(
            restore_bar(Some((-1500, 5000)), BAR, &areas, left, 8),
            Rect::new(-80, 120, 72, 400)
        );
    }

    // ---- collapse / expand anchoring ----

    #[test]
    fn collapsing_keeps_the_center_and_the_edge_nearest_the_monitor_edge() {
        // Right half: the right edge (1908) stays, the vertical center (500) stays.
        let bar = Rect::new(1836, 300, 72, 400);
        assert_eq!(anchor_resize(bar, TAB, AREA), Rect::new(1862, 477, 46, 46));
        // Left half: the left edge stays.
        let bar = Rect::new(10, 300, 72, 400);
        assert_eq!(anchor_resize(bar, TAB, AREA), Rect::new(10, 477, 46, 46));
    }

    #[test]
    fn expanding_near_the_bottom_is_pulled_back_inside() {
        let tab = Rect::new(1862, 1000, 46, 46);
        assert_eq!(anchor_resize(tab, BAR, AREA), Rect::new(1836, 640, 72, 400));
    }

    #[test]
    fn an_unchanged_size_moves_nothing() {
        let bar = Rect::new(900, 301, 72, 400);
        assert_eq!(anchor_resize(bar, BAR, AREA), bar);
        let tab = Rect::new(900, 301, 46, 46);
        assert_eq!(anchor_resize(tab, TAB, AREA), tab);
    }

    // ---- window placement ----

    #[test]
    fn grows_left_when_there_is_room() {
        let bar = Rect::new(1000, 300, 72, 400);
        let p = place(bar, BAR, (170, 0), AREA);
        assert_eq!(p.side, Side::Left);
        assert_eq!(p.window, Rect::new(830, 300, 242, 400));
        assert_eq!(p.bar, bar);
        assert_eq!(p.bar_offset, Point { x: 170, y: 0 });
    }

    #[test]
    fn grows_right_when_there_is_no_room_on_the_left() {
        let bar = Rect::new(50, 300, 72, 400);
        let p = place(bar, BAR, (170, 0), AREA);
        assert_eq!(p.side, Side::Right);
        assert_eq!(p.window, Rect::new(50, 300, 242, 400));
        assert_eq!(p.bar_offset, Point { x: 0, y: 0 });
    }

    #[test]
    fn exactly_enough_room_on_the_left_still_grows_left() {
        let p = place(Rect::new(170, 300, 72, 400), BAR, (170, 0), AREA);
        assert_eq!(p.side, Side::Left);
        assert_eq!(p.window.x, 0);
        let p = place(Rect::new(169, 300, 72, 400), BAR, (170, 0), AREA);
        assert_eq!(p.side, Side::Right);
    }

    #[test]
    fn without_extra_content_the_window_is_the_bar() {
        let bar = Rect::new(700, 200, 72, 400);
        let p = place(bar, BAR, (0, 0), AREA);
        assert_eq!(p.window, bar);
        assert_eq!(p.bar_offset, Point { x: 0, y: 0 });
    }

    #[test]
    fn a_taller_menu_grows_around_the_tab_and_stays_inside_vertically() {
        // Middle of the screen: centered on the tab (center y 523).
        let p = place(Rect::new(1000, 500, 46, 46), TAB, (170, 337), AREA);
        assert_eq!(p.window, Rect::new(830, 355, 216, 337));
        assert_eq!(p.bar_offset, Point { x: 170, y: 145 });
        // Near the bottom: pushed up so nothing is drawn below the work area.
        let p = place(Rect::new(1000, 990, 46, 46), TAB, (170, 337), AREA);
        assert_eq!(p.window.y, 703);
        assert_eq!(p.bar.y, 990);
        assert_eq!(p.bar_offset.y, 287);
        // Near the top: pushed down.
        let p = place(Rect::new(1000, 5, 46, 46), TAB, (170, 337), AREA);
        assert_eq!(p.window.y, 0);
        assert_eq!(p.bar_offset.y, 5);
    }

    #[test]
    fn a_window_taller_than_the_area_is_cut_to_it() {
        let p = place(Rect::new(1000, 300, 72, 400), BAR, (170, 5000), AREA);
        assert_eq!((p.window.y, p.window.height), (0, 1040));
    }

    #[test]
    fn with_no_room_on_either_side_it_takes_the_bigger_side_and_narrows() {
        let narrow = Rect::new(0, 0, 200, 1040);
        // 64 left, 64 right: a tie goes left. Only 64 of the 170 requested fit.
        let p = place(Rect::new(64, 300, 72, 400), BAR, (170, 0), narrow);
        assert_eq!(p.side, Side::Left);
        assert_eq!(p.window, Rect::new(0, 300, 136, 400));
        // More room on the right.
        let p = place(Rect::new(40, 300, 72, 400), BAR, (170, 0), narrow);
        assert_eq!(p.side, Side::Right);
        assert_eq!(p.window, Rect::new(40, 300, 160, 400));
    }

    #[test]
    fn multi_monitor_areas_with_negative_origins() {
        let left = Rect::new(-1920, -200, 1920, 1040);
        let p = place(Rect::new(-1000, 100, 72, 400), BAR, (320, 0), left);
        assert_eq!(p.side, Side::Left);
        assert_eq!(p.window, Rect::new(-1320, 100, 392, 400));
        let p = place(Rect::new(-1900, 100, 72, 400), BAR, (320, 0), left);
        assert_eq!(p.side, Side::Right);
        assert_eq!(p.window, Rect::new(-1900, 100, 392, 400));
    }

    #[test]
    fn a_bar_that_is_partly_off_the_area_is_clamped_first() {
        let p = place(Rect::new(1900, -30, 72, 400), BAR, (170, 0), AREA);
        assert_eq!(p.bar, Rect::new(1848, 0, 72, 400));
        assert_eq!(p.side, Side::Left);
        assert_bar_fixed(p);
    }

    /// Whatever the bar's position and what is open, the bar never moves and
    /// nothing leaves the work area.
    #[test]
    fn the_bar_stays_put_and_the_window_stays_inside_everywhere() {
        let extras = [(0, 0), (170, 0), (170, 337), (320, 0)];
        for &size in &[BAR, TAB] {
            for x in (0..=1848).step_by(97) {
                for y in (0..=990).step_by(83) {
                    for &extra in &extras {
                        let bar = clamp_into(Rect::new(x, y, size.0, size.1), AREA);
                        let p = place(bar, size, extra, AREA);
                        assert_eq!(p.bar, bar, "the bar itself must not move ({x},{y}) {extra:?}");
                        assert_bar_fixed(p);
                        assert_inside(p.window, AREA);
                    }
                }
            }
        }
    }
}
