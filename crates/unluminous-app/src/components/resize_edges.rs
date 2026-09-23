//! The eight places the window itself is resized from.
//!
//! Unluminous draws its own title bar, which means the window is created with `with_decorations(false)`, and
//! an undecorated window has no frame for the operating system to offer a resize grip on. `task-1658`
//! is what that cost: the window could be resized from the top, where the title bar's own drag happened
//! to land on something the platform still handled, and from nowhere else.
//!
//! So the eight grips are drawn here — four edges and four corners — as invisible strips inside the
//! window's own rectangle. Each one sets the pointer to the arrow for the direction it moves and, when it
//! is dragged, sends `ViewportCommand::BeginResize`, which hands the drag to the window manager. Nothing
//! is painted: the window already has its rounded rectangle, and a visible frame is exactly what turning
//! the decorations off was for.
//!
//! **They are added to the `Ui` last**, after every pane, for the reason `components::splitter` records
//! about dividers: a widget added earlier sits underneath one added later, and the editing area, the
//! explorer and the status bar all take drags over the whole of their rectangles. Added first, the grips
//! never saw a pointer.
//!
//! A corner is a square [`CORNER`] points on a side and an edge is [`EDGE`] points wide. The corner has
//! to win where the two overlap, so the corners are added after the edges.
//!
//! ## A maximised window has no grips at all, and that is not a nicety
//!
//! `task-1693` reported a window that could not be resized. Driven with real mouse input, a freshly
//! started Unluminous resizes from all four edges and all four corners, and in `egui_kittest` all eight
//! report `drag_started`. What is broken is what happens when the window manager **refuses** the
//! request.
//!
//! `ViewportCommand::BeginResize` becomes winit's `handle_os_dragging`, which latches a private
//! `dragging` flag, posts `WM_NCLBUTTONDOWN`, and returns early from **every later call** until that
//! flag is cleared. The only place in winit that clears it is `WM_EXITSIZEMOVE` — the end of a modal
//! size or move loop. A posted `WM_NCLBUTTONDOWN` that never starts such a loop therefore latches
//! the flag for the life of the process, and a **maximised** window is exactly that case: Windows
//! turns the hit test into `SC_SIZE`, and `Size` is disabled on a maximised window. One refused edge
//! drag and the window can no longer be resized **or moved** at all, because the title bar's own
//! `StartDrag` goes through the same latch. Measured twice on this machine: after a single edge drag
//! on a maximised window, every later drag did nothing, and a freshly started Unluminous worked at once.
//!
//! So **no grip is added while the window is maximised**. That is Unluminous's own rule for a control
//! that can never apply — a maximised window has no size to change — and it is what makes it
//! impossible to send a request the window manager will throw away. The title bar's `StartDrag` is
//! left alone, because Windows *does* handle dragging a maximised window: it restores it and moves
//! it, which is a real modal loop and therefore an honest `WM_EXITSIZEMOVE`.
//!
//! ## macOS has no window manager drag to hand the gesture to, so the window moves its own edge
//!
//! Everything above is about `BeginResize` being *refused*. On macOS it is not refused, it is **not
//! implemented at all**: `winit`'s `WindowDelegate::drag_resize_window` is three lines that return
//! `NotSupported` for every one of the eight directions, and `egui-winit` logs that at `warn` and
//! carries on. So a grip here sent a request that had never once resized this window, and the report
//! was the plain truth — the arrow appears on hover and the drag does nothing.
//!
//! It was also **two** faults wearing one symptom, because the grips are added last and take the
//! outermost [`EDGE`] points of the window. `EDGE_CURSORS` sets `ResizeHorizontal`, which is the same
//! cursor `components::splitter` sets, so where a pane divider reaches the window's edge the two
//! controls are indistinguishable and the one on top was the dead one. That is the other half of the
//! report: *"when I can't resize a pane"*. Measured on a real window, a strip divider 2 and 5 points
//! in from the right edge moved nothing while the same divider 7 points in moved 47 points.
//!
//! So on macOS a drag is applied here rather than handed over. [`Gesture`] is what a grip reports:
//! [`Gesture::Begin`] is the request the platforms that implement it want, and [`Gesture::Move`] is
//! the pointer's movement this frame, which the caller turns into a new position and size through
//! `ViewportCommand::InnerSize` and `OuterPosition` — the two commands `unluminous-cli window size`
//! and `window position` already drive this window with, so this is not a second mechanism. Dragging
//! a north or a west edge has to move the window as well as resize it, or the far edge would travel.

use egui::viewport::ResizeDirection;
use egui::{Pos2, Rect, Sense, Vec2};

/// How far in from an edge the window can be grabbed.
///
/// Six points, for the reason `splitter::GRAB` is eight: a one point target cannot be hit with a mouse.
/// Six rather than eight because these grips are added last and so sit **over** everything at the
/// window's edge, and six is what the activity bar's buttons are inset by — so a button and a grip never
/// want the same point.
pub const EDGE: f32 = 6.0;
/// How far along each edge a corner reaches, which is roughly what a window manager offers.
pub const CORNER: f32 = 16.0;

// Both constants, so a runtime `assert!` of one against the other can only ever pass or fail the
// same way. Checked at compile time instead: a corner has to reach further than an edge so it can
// win where they overlap, and a build fails before it ever draws one that could not.
const _: () = assert!(CORNER > EDGE, "a corner has to reach further than an edge");

/// What a grip was asked to do, which is not the same question on every platform.
///
/// See the note at the top of this file. Two platforms have a window manager drag to hand the whole
/// gesture to and one has not, so a grip reports either the request or the movement rather than the
/// caller having to know which it is looking at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Hand the whole drag to the window manager, which then owns the pointer until it is let go.
    ///
    /// Reported once, on the frame the drag began: sending it again on the next frame would ask for a
    /// second resize inside the first.
    Begin(ResizeDirection),
    /// Move this edge by `by` points, which is how far the pointer moved since the last frame.
    ///
    /// Reported on **every** frame of the drag, because there is nothing else moving the edge. Only
    /// the component along the direction's own axis is read — the caller decides what an edge moving
    /// means for the window's position and size, which is `Edges::moved_by`.
    Move { direction: ResizeDirection, by: Vec2 },
}

impl Gesture {
    /// Which edge or corner the gesture is about, whichever shape it took.
    pub fn direction(self) -> ResizeDirection {
        match self {
            Self::Begin(direction) | Self::Move { direction, .. } => direction,
        }
    }
}

/// Whether this platform resizes a window by handing the drag to the window manager.
///
/// **False on macOS, and that is measured rather than assumed**: `winit`'s
/// `WindowDelegate::drag_resize_window` returns `NotSupported` for every direction there, so
/// `BeginResize` has never once resized this window. See the note at the top of this file.
pub const HANDS_THE_DRAG_OVER: bool = !cfg!(target_os = "macos");

/// Add the eight grips over `window`, and report what one of them was asked to do.
///
/// The caller sends the viewport commands, so this component changes nothing itself, which is the rule
/// every component in Unluminous follows.
///
/// `maximized` says whether the window is maximised, and when it is **nothing is added at all** — no
/// grip, no cursor, no request. See the note at the top of this file: a resize the window manager
/// refuses does not merely fail, it wedges every later move and resize as well.
///
/// `keep` is the controls that must keep their own points, which is every pane divider. **A grip gives
/// those points up rather than covering them**, and that is the second half of `task-2062`: the grips
/// are added last, so a divider reaching the window's edge was underneath one — and both set the same
/// double headed cursor, so the two were indistinguishable and the one on top was the window's. A
/// divider two points in from the window's right edge moved nothing while the same divider seven
/// points in moved normally. See [`without`] for how a strip gives up a run of itself.
pub fn show(ui: &mut egui::Ui, window: Rect, maximized: bool, keep: &[Rect]) -> Option<Gesture> {
    if maximized {
        return None;
    }
    let mut started = None;
    // **The edges use the double-headed cursors, not the one-way ones.** `task-1848`: "there's one on the
    // left edge that points left with a bar, that doesn't do anything when I drag and click once it's
    // shown. The two arrow icon shows, and that allows me to resize."
    //
    // Both halves of that report are about the same four points of overlap. `ResizeWest` is drawn by macOS
    // as an arrow against a bar, and it is what these edges used to set; `components::splitter` — the
    // divider between two panes, a few points away — sets `ResizeHorizontal`, the double-headed arrow, and
    // that one is grabbed by the splitter rather than by the window. So the two cursors marked two
    // different controls sitting on top of each other, and the one-way arrow was the window's edge, whose
    // drag then went to the window manager rather than moving the divider the person was aiming at.
    //
    // A double-headed arrow says "this edge moves in two directions", which is true of a window edge and
    // of a divider alike, so both now look the same and the difference is which one is under the pointer
    // rather than which glyph is showing. The corners keep their diagonals: those are unambiguous and
    // there is nothing else at a corner to confuse them with.
    //
    // The grips themselves stay. An undecorated window has no frame for the platform to put a grip on —
    // `task-1658` measured a window that could only be resized from the top, where the title bar's drag
    // happened to land on something the platform still handled — so removing them would take resizing
    // away rather than tidying it up.
    //
    // The four edges first, then the four corners over them, so a grab in a corner resizes both ways.
    let edges: [(&str, ResizeDirection, Rect, egui::CursorIcon); 4] = [
        (
            "top",
            ResizeDirection::North,
            Rect::from_min_size(window.left_top(), Vec2::new(window.width(), EDGE)),
            EDGE_CURSORS[0],
        ),
        (
            "bottom",
            ResizeDirection::South,
            Rect::from_min_size(
                egui::pos2(window.left(), window.bottom() - EDGE),
                Vec2::new(window.width(), EDGE),
            ),
            EDGE_CURSORS[1],
        ),
        (
            "left",
            ResizeDirection::West,
            Rect::from_min_size(window.left_top(), Vec2::new(EDGE, window.height())),
            EDGE_CURSORS[2],
        ),
        (
            "right",
            ResizeDirection::East,
            Rect::from_min_size(
                egui::pos2(window.right() - EDGE, window.top()),
                Vec2::new(EDGE, window.height()),
            ),
            EDGE_CURSORS[3],
        ),
    ];
    let corners: [(&str, ResizeDirection, egui::Pos2, egui::CursorIcon); 4] = [
        (
            "top left",
            ResizeDirection::NorthWest,
            window.left_top(),
            egui::CursorIcon::ResizeNorthWest,
        ),
        (
            "top right",
            ResizeDirection::NorthEast,
            window.right_top() - Vec2::new(CORNER, 0.0),
            egui::CursorIcon::ResizeNorthEast,
        ),
        (
            "bottom left",
            ResizeDirection::SouthWest,
            window.left_bottom() - Vec2::new(0.0, CORNER),
            egui::CursorIcon::ResizeSouthWest,
        ),
        (
            "bottom right",
            ResizeDirection::SouthEast,
            window.right_bottom() - Vec2::splat(CORNER),
            egui::CursorIcon::ResizeSouthEast,
        ),
    ];

    // The corners are added over the edges, so the four corner squares are what an edge gives up
    // besides whatever else asked to keep its points. Collected first because an edge is cut against
    // them and they are the same four squares whichever edge is asking.
    let corner_squares: Vec<Rect> =
        corners.iter().map(|(_, _, at, _)| Rect::from_min_size(*at, Vec2::splat(CORNER))).collect();

    for (name, direction, area, cursor) in edges {
        let upright = matches!(direction, ResizeDirection::West | ResizeDirection::East);
        // An edge gives up the runs of itself that a divider wants, and becomes the pieces that are
        // left. A corner is not cut away here: it is added afterwards and wins by being later, which
        // is the rule this file already kept.
        let pieces = without(area, keep, upright);
        // **A piece is a control of its own, so it needs its own id and its own name.** `egui`
        // identifies a widget by its id, so pieces sharing one id are one widget wearing several
        // rectangles and the last `interact` wins — which left every piece but the final one dead.
        // Measured on a real window: with the terminal along the bottom, the right edge resized below
        // the terminal's divider and did nothing above it. And `no_two_controls_share_a_name` is the
        // other half of the same point: two controls with one name is what the style guide forbids and
        // what the screenshot tests find controls by.
        //
        // **An edge nothing crosses keeps its plain name**, `Resize window: right`, which is what the
        // tests already look it up by and what a person would call it. Only where an edge really came
        // apart is a piece numbered, and then every piece is — `right 1 of 2` rather than one plain name
        // and one numbered, because a name that means "the whole edge" for one piece and "this part of
        // it" for another is worse than either.
        let several = pieces.len() > 1;
        let count = pieces.len();
        for (which, piece) in pieces.into_iter().enumerate() {
            let numbered = format!("{name} {} of {count}", which + 1);
            let named = if several { numbered.as_str() } else { name };
            if let Some(gesture) = grip_at(ui, piece, named, which, cursor, direction) {
                started = Some(gesture);
            }
        }
    }
    for ((name, direction, _, cursor), area) in corners.into_iter().zip(corner_squares) {
        // **A corner keeps its whole square.** A corner is 16 points of two edges meeting and is the
        // one place a window is resized in both directions at once; a divider crossing it is at the
        // very end of its own run, where there is nothing left to resize on the far side. Cutting the
        // corners as well left the window with no way to be resized diagonally at all wherever a panel
        // happened to reach one.
        if let Some(gesture) = grip(ui, area, name, cursor, direction) {
            started = Some(gesture);
        }
    }
    started
}

/// The pieces of `strip` that are left once every rectangle in `keep` has been taken out of it.
///
/// A window edge is a long thin strip and the things that want a piece of it — pane dividers — cross
/// it, so what is left is a run of shorter strips. `upright` says which way the strip runs: a left or
/// right edge is cut along y, and a top or bottom edge along x.
///
/// **Only a rectangle that really overlaps takes anything**, and a piece narrower than
/// [`SMALLEST_PIECE`] is dropped rather than added: a two point grip cannot be hit with a mouse, and
/// adding one is a widget that takes a hover and answers no drag, which is the fault this whole
/// function exists to fix wearing a smaller size.
pub fn without(strip: Rect, keep: &[Rect], upright: bool) -> Vec<Rect> {
    // Where along the strip each kept rectangle starts and ends, in the strip's own long direction.
    let mut taken: Vec<(f32, f32)> = keep
        .iter()
        .filter(|other| other.intersects(strip))
        .map(|other| match upright {
            true => (other.top(), other.bottom()),
            false => (other.left(), other.right()),
        })
        .collect();
    if taken.is_empty() {
        return vec![strip];
    }
    taken.sort_by(|a, b| a.0.total_cmp(&b.0));

    let (from, to) = match upright {
        true => (strip.top(), strip.bottom()),
        false => (strip.left(), strip.right()),
    };
    let piece = |start: f32, end: f32| match upright {
        true => Rect::from_min_max(Pos2::new(strip.left(), start), Pos2::new(strip.right(), end)),
        false => Rect::from_min_max(Pos2::new(start, strip.top()), Pos2::new(end, strip.bottom())),
    };

    let mut pieces = Vec::new();
    let mut at = from;
    for (start, end) in taken {
        if start - at >= SMALLEST_PIECE {
            pieces.push(piece(at, start));
        }
        at = at.max(end);
    }
    if to - at >= SMALLEST_PIECE {
        pieces.push(piece(at, to));
    }
    pieces
}

/// The shortest run of a window edge still worth adding a grip over.
///
/// Eight points, which is [`crate::components::splitter::GRAB`] — the width that file gives as the
/// smallest thing a mouse can be asked to hit. A shorter piece is left out entirely rather than added,
/// because a grip nobody can hit is a rectangle that takes the hover and the cursor off whatever is
/// underneath it and then does nothing, which is exactly the fault `without` exists to fix.
pub const SMALLEST_PIECE: f32 = crate::components::splitter::GRAB;

/// Which of the window's four edges a direction moves, which is the whole of what a resize is.
///
/// Split out from [`Edges::moved_by`] so that the two halves — which edges move, and what moving them
/// does to a rectangle — are each a test of their own. A corner moves one of each pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    /// The edges `direction` moves.
    pub fn of(direction: ResizeDirection) -> Self {
        use ResizeDirection::*;
        let left = matches!(direction, West | NorthWest | SouthWest);
        let right = matches!(direction, East | NorthEast | SouthEast);
        let top = matches!(direction, North | NorthWest | NorthEast);
        let bottom = matches!(direction, South | SouthWest | SouthEast);
        Self { left, right, top, bottom }
    }

    /// Where the window goes and how large it becomes when these edges move by `by`.
    ///
    /// **Both halves, because moving a north or a west edge moves the window too.** Dragging the top
    /// edge down by ten points means a window ten points shorter whose top is ten points lower — read
    /// as a size alone, the *bottom* edge would have travelled instead, which is the far edge of the
    /// one being dragged and is not what anybody is aiming at.
    ///
    /// `smallest` is the window's own minimum, and it is applied **before** the position so that an
    /// edge dragged past it stops rather than dragging the window along behind a size that cannot
    /// shrink any further. Without that, pushing a top edge down through the floor slid the whole
    /// window down the screen while its height stood still.
    pub fn moved_by(self, window: Rect, by: Vec2, smallest: Vec2) -> (egui::Pos2, Vec2) {
        let dx = f32::from(self.right) * by.x - f32::from(self.left) * by.x;
        let dy = f32::from(self.bottom) * by.y - f32::from(self.top) * by.y;
        let size = Vec2::new(
            (window.width() + dx).max(smallest.x),
            (window.height() + dy).max(smallest.y),
        );
        // How much the size really changed, which is what the moving edge may spend: a drag that hit
        // the floor moved the edge less than the pointer asked for.
        let spent = size - window.size();
        let at = egui::Pos2::new(
            window.left() - f32::from(self.left) * spent.x,
            window.top() - f32::from(self.top) * spent.y,
        );
        (at, size)
    }
}

/// Whether a drag on a grip becomes a request to the window manager.
///
/// **The one decision `app::frame::show_the_resize_grips` makes**, here rather than inline so that a
/// test can hold both answers side by side: `BeginResize` goes straight to the window manager and
/// nothing inside this process can watch a window change size.
///
/// `page_has_the_keyboard` is whether a native child — a browser node's page — has taken the operating
/// system's keyboard focus. While one has, `winit`'s `handle_os_dragging` latches a flag that only
/// `WM_EXITSIZEMOVE` clears and returns early from every later move and resize for the life of the
/// process; see the note at the top of this file for what that costs.
///
/// **It is that question rather than "does the window have the focus"**, which is what `task-1945`
/// asked and what `task-2004` found was refusing every resize for a second reason: a window merely in
/// the background reports no focus either, and there the eight grips were dead for no reason at all.
pub fn ask_for_it(gesture: Option<Gesture>, page_has_the_keyboard: bool) -> Option<Gesture> {
    gesture.filter(|_| !page_has_the_keyboard)
}

/// One grip: invisible, named, and reporting whichever shape of gesture this platform needs.
///
/// Where the window manager takes the drag, it is reported **once**, on the frame it started: it then
/// owns the pointer until it is let go, and sending it again on the next frame would ask for a second
/// resize inside the first. Where it does not — macOS — the movement is reported on every frame,
/// because nothing else is moving the edge.
fn grip(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    cursor: egui::CursorIcon,
    direction: ResizeDirection,
) -> Option<Gesture> {
    grip_at(ui, area, name, 0, cursor, direction)
}

/// One piece of a grip: the same control, told apart from the other pieces of the same edge.
///
/// `which` is the piece's place along the edge and reaches the **id** only. Two rectangles interacted
/// with under one id are one widget in egui's eyes and the last one wins, which is what left every
/// piece of a cut edge dead but the final one. The **name** stays the edge's own, because a name is
/// what a person and a test call a control by and all the pieces are one control.
fn grip_at(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    which: usize,
    cursor: egui::CursorIcon,
    direction: ResizeDirection,
) -> Option<Gesture> {
    let response = ui.interact(area, ui.id().with(("resize-window", name, which)), Sense::drag());
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(cursor);
    }
    // Every control in Unluminous has a plain name, so a test can find this one without a pointer.
    let label = format!("Resize window: {name}");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), label.clone())
    });
    match HANDS_THE_DRAG_OVER {
        true => response.drag_started().then_some(Gesture::Begin(direction)),
        false => {
            let by = response.drag_delta();
            // A frame of a drag in which the pointer did not move is not a movement, and asking for the
            // size it already has would be a `setContentSize` a frame for nothing.
            (response.dragged() && by != Vec2::ZERO).then_some(Gesture::Move { direction, by })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page holding the operating system's keyboard is the one thing that stops a resize being
    /// asked for, and a window that is merely in the background is not that thing. `task-2004`.
    #[test]
    fn a_resize_is_asked_for_unless_a_page_has_taken_the_keyboard() {
        let north = Some(Gesture::Begin(ResizeDirection::North));
        assert_eq!(ask_for_it(north, false), north, "the ordinary case");
        assert_eq!(ask_for_it(north, true), None, "a browser node's page has the keyboard");
        assert_eq!(ask_for_it(None, false), None, "and no drag asks for nothing");
    }

    /// Which edges each of the eight directions moves. A corner moves one of each pair, and no
    /// direction ever moves two edges that face each other.
    #[test]
    fn a_direction_moves_the_edges_its_name_says() {
        use ResizeDirection::*;
        assert_eq!(Edges::of(North), Edges { top: true, ..Default::default() });
        assert_eq!(Edges::of(South), Edges { bottom: true, ..Default::default() });
        assert_eq!(Edges::of(West), Edges { left: true, ..Default::default() });
        assert_eq!(Edges::of(East), Edges { right: true, ..Default::default() });
        assert_eq!(Edges::of(NorthWest), Edges { top: true, left: true, ..Default::default() });
        assert_eq!(Edges::of(SouthEast), Edges { bottom: true, right: true, ..Default::default() });
        for direction in [North, South, East, West, NorthEast, NorthWest, SouthEast, SouthWest] {
            let edges = Edges::of(direction);
            assert!(!(edges.left && edges.right), "{direction:?} moves two edges that face");
            assert!(!(edges.top && edges.bottom), "{direction:?} moves two edges that face");
        }
    }

    /// **A far edge never travels**, which is the whole of what makes a drag feel like a drag: the
    /// edge under the pointer is the one that moves, so the other three stay where they are.
    #[test]
    fn dragging_an_edge_leaves_the_other_three_where_they_were() {
        let window = Rect::from_min_size(egui::pos2(100.0, 50.0), Vec2::new(800.0, 600.0));
        let smallest = Vec2::new(320.0, 240.0);
        let after = |direction, by| {
            let (at, size) = Edges::of(direction).moved_by(window, by, smallest);
            Rect::from_min_size(at, size)
        };

        // The right edge out by 40: the left, the top and the bottom are untouched.
        let east = after(ResizeDirection::East, Vec2::new(40.0, 0.0));
        assert_eq!(east.min, window.min, "dragging the right edge moved the window");
        assert_eq!(east.width(), 840.0);
        assert_eq!(east.height(), window.height(), "and it changed the height");

        // The left edge out by 40, which is a *negative* movement: the right edge must not move.
        let west = after(ResizeDirection::West, Vec2::new(-40.0, 0.0));
        assert_eq!(west.left(), 60.0, "the left edge did not follow the pointer");
        assert_eq!(west.right(), window.right(), "the right edge travelled");
        assert_eq!(west.width(), 840.0);

        // And the top edge down by 30, which is the pair of the case above on the other axis.
        let north = after(ResizeDirection::North, Vec2::new(0.0, 30.0));
        assert_eq!(north.top(), 80.0);
        assert_eq!(north.bottom(), window.bottom(), "the bottom edge travelled");
        assert_eq!(north.height(), 570.0);

        // A corner moves two edges and leaves the opposite corner alone.
        let corner = after(ResizeDirection::NorthWest, Vec2::new(-25.0, -15.0));
        assert_eq!(corner.max, window.max, "the opposite corner moved");
        assert_eq!(corner.size(), Vec2::new(825.0, 615.0));
    }

    /// An edge dragged past the smallest the window may be **stops**, and does not drag the window
    /// along behind a size that cannot shrink any further.
    #[test]
    fn an_edge_dragged_past_the_smallest_size_stops_rather_than_sliding_the_window() {
        let window = Rect::from_min_size(egui::pos2(100.0, 50.0), Vec2::new(400.0, 300.0));
        let smallest = Vec2::new(320.0, 240.0);

        // The top edge pushed down by 400, where only 60 of it is available.
        let (at, size) =
            Edges::of(ResizeDirection::North).moved_by(window, Vec2::new(0.0, 400.0), smallest);
        assert_eq!(size, Vec2::new(400.0, 240.0), "the height went under the floor");
        assert_eq!(at.y, 110.0, "the window slid further than the edge could move");
        assert_eq!(at.y + size.y, window.bottom(), "and the bottom edge travelled");

        // The same on the other axis and the other side of the window.
        let (at, size) =
            Edges::of(ResizeDirection::West).moved_by(window, Vec2::new(400.0, 0.0), smallest);
        assert_eq!(size.x, 320.0);
        assert_eq!(at.x + size.x, window.right(), "the right edge travelled");
    }

    /// **A pane divider keeps its own points**, which is the second half of `task-2062`. Measured on a
    /// real window before the fix: a strip divider 2 and 5 points in from the window's right edge moved
    /// nothing, while the same divider 7 points in moved 47 points.
    #[test]
    fn a_window_edge_gives_up_the_points_a_divider_wants() {
        // A right edge: 6 points wide, running the height of the window.
        let edge = Rect::from_min_max(egui::pos2(794.0, 0.0), egui::pos2(800.0, 600.0));
        // A flat divider crossing it 300 points down, grabbed over 8 points.
        let divider = Rect::from_min_max(egui::pos2(36.0, 296.0), egui::pos2(800.0, 304.0));

        let pieces = without(edge, std::slice::from_ref(&divider), true);
        assert_eq!(pieces.len(), 2, "the edge did not come apart: {pieces:?}");
        assert_eq!(pieces[0], Rect::from_min_max(egui::pos2(794.0, 0.0), egui::pos2(800.0, 296.0)));
        assert_eq!(
            pieces[1],
            Rect::from_min_max(egui::pos2(794.0, 304.0), egui::pos2(800.0, 600.0))
        );
        // No piece holds any of the divider's own points. Asked as an overlap of area rather than
        // through `Rect::intersects`, which counts two rectangles that merely touch along an edge — and
        // abutting exactly at 296 is what giving the points up looks like.
        for piece in &pieces {
            let shared = piece.intersect(divider);
            assert!(
                shared.height() <= 0.0,
                "{piece:?} still holds {}pt of the divider",
                shared.height()
            );
        }

        // Nothing in the way leaves the edge whole, which is what every window without a panel does.
        assert_eq!(without(edge, &[], true), vec![edge]);
        // And a rectangle nowhere near it takes nothing.
        let elsewhere = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 20.0));
        assert_eq!(without(edge, &[elsewhere], true), vec![edge]);
    }

    /// Two dividers crossing one edge, and one that overlaps the end of it: the pieces are still in
    /// order, none of them covers a divider, and a sliver too small to hit is dropped rather than added.
    #[test]
    fn several_dividers_leave_the_pieces_between_them() {
        let edge = Rect::from_min_max(egui::pos2(0.0, 794.0), egui::pos2(1000.0, 800.0));
        let upright = false;
        let dividers = [
            // Two crossing it, out of order on purpose: the cut sorts them.
            Rect::from_min_max(egui::pos2(596.0, 38.0), egui::pos2(604.0, 800.0)),
            Rect::from_min_max(egui::pos2(296.0, 38.0), egui::pos2(304.0, 800.0)),
            // And one four points from the end, so the piece after it is too small to be worth adding.
            Rect::from_min_max(egui::pos2(992.0, 38.0), egui::pos2(1000.0, 800.0)),
        ];

        let pieces = without(edge, &dividers, upright);
        assert_eq!(pieces.len(), 3, "{pieces:?}");
        let spans: Vec<(f32, f32)> = pieces.iter().map(|p| (p.left(), p.right())).collect();
        assert_eq!(spans, vec![(0.0, 296.0), (304.0, 596.0), (604.0, 992.0)]);
        for piece in &pieces {
            assert!(piece.width() >= SMALLEST_PIECE, "a piece nobody can hit: {piece:?}");
            for divider in &dividers {
                let shared = piece.intersect(*divider);
                assert!(shared.width() <= 0.0, "{piece:?} holds part of {divider:?}");
            }
        }
    }

    /// A divider running the whole length of an edge takes all of it, and the answer is no grip rather
    /// than a grip of no size. Nothing is ever asked to interact with an empty rectangle.
    #[test]
    fn a_divider_along_the_whole_edge_leaves_no_grip_at_all() {
        let edge = Rect::from_min_max(egui::pos2(794.0, 0.0), egui::pos2(800.0, 600.0));
        let all_of_it = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));
        assert!(without(edge, &[all_of_it], true).is_empty());
    }

    /// **A corner keeps its whole square**, whatever crosses it. A corner is the one place the window is
    /// resized in both directions at once, and cutting them as well left a window with a panel reaching a
    /// corner unable to be resized diagonally at all.
    #[test]
    fn the_corners_are_not_cut_against_anything() {
        let context = egui::Context::default();
        let window = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(800.0, 600.0));
        // A divider through every corner of the window, which is the worst case for the rule above: the
        // four edges give nearly all of themselves up and the four corners give up nothing.
        let across = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));

        // A widget is under the pointer at each of the four corners. Asked as a **hover** rather than by
        // driving a drag, because what the drag reports differs by platform — `HANDS_THE_DRAG_OVER` — and
        // what this test is about is whether a grip is there at all.
        for (name, at) in [
            ("top left", window.left_top() + Vec2::splat(2.0)),
            ("top right", window.right_top() + Vec2::new(-2.0, 2.0)),
            ("bottom left", window.left_bottom() + Vec2::new(2.0, -2.0)),
            ("bottom right", window.right_bottom() - Vec2::splat(2.0)),
        ] {
            // **Two passes at the same position.** `egui` works a hover out from the widget rectangles of
            // the *previous* pass, so the first one has nothing to compare the pointer against and
            // reports no hover anywhere. The same rule `services::input` records about a click being
            // three frames.
            let mut cursor = egui::CursorIcon::Default;
            for _ in 0..2 {
                let input = egui::RawInput {
                    events: vec![egui::Event::PointerMoved(at)],
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let _ = show(ui, window, false, &[across]);
                });
                output.textures_delta.clear();
                // A grip sets its own cursor while it is hovered, and a corner's is a diagonal. Nothing
                // else in this pass draws anything, so a diagonal cursor is the corner's grip.
                cursor = output.platform_output.cursor_icon;
            }
            assert!(
                matches!(
                    cursor,
                    egui::CursorIcon::ResizeNorthWest
                        | egui::CursorIcon::ResizeNorthEast
                        | egui::CursorIcon::ResizeSouthWest
                        | egui::CursorIcon::ResizeSouthEast
                ),
                "the {name} corner was cut away: the cursor there is {cursor:?}"
            );
        }
    }

    /// **Every piece of a cut edge is live, not only the last one.** `egui` identifies a widget by its
    /// id, so the pieces sharing one id were one widget wearing several rectangles and the last
    /// `interact` won. Measured on a real window with the terminal along the bottom: the right edge
    /// resized below the terminal's divider and did nothing at all above it, which reads exactly like
    /// the fault this whole change is about and was introduced by the fix for it.
    ///
    /// Asked through the **cursor**, which a grip sets while it is hovered, because that is the one
    /// thing a widget with no drag in flight reports and it is the same question `task-1848` measured
    /// the edges' own arrows with.
    #[test]
    fn every_piece_of_a_cut_edge_answers_the_pointer() {
        let context = egui::Context::default();
        let window = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(800.0, 600.0));
        // A flat divider crossing the right edge half way down, which is the terminal's own shape.
        let divider = Rect::from_min_max(egui::pos2(36.0, 296.0), egui::pos2(800.0, 304.0));
        assert_eq!(
            without(
                Rect::from_min_max(egui::pos2(794.0, 0.0), egui::pos2(800.0, 600.0)),
                std::slice::from_ref(&divider),
                true,
            )
            .len(),
            2,
            "the fixture does not cut the edge in two, so this test is about nothing"
        );

        // Above the divider and below it. Both are the right edge and both have to answer.
        for (where_it_is, at) in [("above the divider", 150.0), ("below the divider", 450.0)] {
            let mut cursor = egui::CursorIcon::Default;
            // Two passes, because egui works a hover out from the previous pass's rectangles.
            for _ in 0..2 {
                let input = egui::RawInput {
                    events: vec![egui::Event::PointerMoved(egui::pos2(797.0, at))],
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let _ = show(ui, window, false, std::slice::from_ref(&divider));
                });
                output.textures_delta.clear();
                cursor = output.platform_output.cursor_icon;
            }
            assert_eq!(
                cursor,
                egui::CursorIcon::ResizeHorizontal,
                "the piece of the right edge {where_it_is} is dead"
            );
        }
    }

    /// **An edge nothing crosses keeps its plain name, and the pieces of a cut one are each named.**
    /// Two controls with one name is what `design/style-guide.md` forbids and what the screenshot tests
    /// find controls by, so a cut edge cannot leave two grips both called `Resize window: right`.
    #[test]
    fn a_cut_edge_names_each_of_its_pieces_and_an_uncut_one_keeps_its_plain_name() {
        let window = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(800.0, 600.0));
        // **The names `show` really gave its grips**, read out of the accessibility tree rather than
        // worked out again here: a second copy of the naming rule would pass while the two disagreed,
        // which is the fault `plugins::PANE_ICONS` and `activity_bar::pane_icon` already recorded.
        let names = |keep: &[Rect]| -> Vec<String> {
            let context = egui::Context::default();
            context.enable_accesskit();
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                let _ = show(ui, window, false, keep);
            });
            output.textures_delta.clear();
            let tree = output.platform_output.accesskit_update.take().expect("an accesskit tree");
            tree.nodes
                .iter()
                .filter_map(|(_, node)| node.label())
                .map(|label| label.to_owned())
                .filter(|label| label.starts_with("Resize window"))
                .collect()
        };

        // Nothing in the way: both edges keep the plain names the tests already use.
        let plain = names(&[]);
        assert!(plain.contains(&"Resize window: top".to_owned()), "{plain:?}");
        assert!(plain.contains(&"Resize window: right".to_owned()), "{plain:?}");

        // A divider across the right edge: two pieces, two names, and neither is the plain one.
        let divider = Rect::from_min_max(egui::pos2(36.0, 296.0), egui::pos2(800.0, 304.0));
        let cut = names(std::slice::from_ref(&divider));
        assert!(cut.contains(&"Resize window: right 1 of 2".to_owned()), "{cut:?}");
        assert!(cut.contains(&"Resize window: right 2 of 2".to_owned()), "{cut:?}");
        assert!(!cut.contains(&"Resize window: right".to_owned()), "{cut:?}");
        // And no name appears twice, which is the rule this test exists for.
        let mut once = cut.clone();
        once.sort();
        once.dedup();
        assert_eq!(once.len(), cut.len(), "two controls share a name: {cut:?}");
    }

    /// The shape a grip reports follows the platform, and the reason is measured rather than chosen:
    /// `winit` has no macOS `drag_resize_window`, so there is nothing to hand a drag to there.
    #[test]
    fn a_grip_reports_the_shape_this_platform_can_act_on() {
        assert_eq!(HANDS_THE_DRAG_OVER, !cfg!(target_os = "macos"));
        let gesture = Gesture::Move { direction: ResizeDirection::East, by: Vec2::new(5.0, 0.0) };
        assert_eq!(gesture.direction(), ResizeDirection::East);
        assert_eq!(Gesture::Begin(ResizeDirection::West).direction(), ResizeDirection::West);
    }

    /// `task-1693`: a maximised window adds no grips, so no resize the window manager would refuse
    /// is ever asked for. See the note at the top of this file for what one refused request costs.
    /// That nothing is *drawn* either is checked in the screenshot tests, which can ask the window
    /// for a control by name.
    #[test]
    fn a_maximised_window_asks_for_no_resize() {
        let context = egui::Context::default();
        let mut answer = Some(Gesture::Begin(ResizeDirection::North));
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let window = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(800.0, 600.0));
            answer = show(ui, window, true, &[]);
        });
        // egui insists a pass's texture changes are taken or cleared before the output is dropped.
        output.textures_delta.clear();
        assert_eq!(answer, None, "a maximised window has no size to change");
    }
}

/// The cursor each of the four window edges sets, in the order `edges` builds them: top, bottom, left,
/// right.
///
/// **`edges` reads this rather than spelling the four out**, so the test below asserts the cursors the
/// window really sets. A test against a second copy of the list would pass while the two disagreed, and a
/// cursor cannot be read off a screenshot — it is a value on the frame's output rather than something
/// drawn into the window — so this constant is the only place a test can see them.
pub const EDGE_CURSORS: [egui::CursorIcon; 4] = [
    egui::CursorIcon::ResizeVertical,
    egui::CursorIcon::ResizeVertical,
    egui::CursorIcon::ResizeHorizontal,
    egui::CursorIcon::ResizeHorizontal,
];

#[cfg(test)]
mod cursor_tests {
    use super::*;

    /// `task-1848`: "theres one on the left edge that points left with a bar, that doesn't do anything
    /// when I drag and click once its shown. The two arrow icon shows, and that allows me to resize."
    ///
    /// The one-way arrows are what macOS draws for `ResizeWest` and its three siblings, and they sat on
    /// top of `components::splitter`'s double-headed ones where a pane divider reaches the window's edge.
    /// A change back to a one-way arrow fails here.
    #[test]
    fn the_window_edges_use_the_double_headed_cursors() {
        for cursor in EDGE_CURSORS {
            assert!(
                matches!(
                    cursor,
                    egui::CursorIcon::ResizeVertical | egui::CursorIcon::ResizeHorizontal
                ),
                "an edge set {cursor:?}, which macOS draws as an arrow against a bar"
            );
        }
    }

    /// The corners keep their diagonals: unambiguous, and nothing else is at a corner to confuse them
    /// with. Named so that folding them into the pair above would have to be a deliberate change.
    #[test]
    fn the_corners_keep_their_diagonal_cursors() {
        use egui::CursorIcon::*;
        for cursor in [ResizeNorthWest, ResizeNorthEast, ResizeSouthWest, ResizeSouthEast] {
            assert!(!matches!(cursor, ResizeVertical | ResizeHorizontal));
        }
    }
}
