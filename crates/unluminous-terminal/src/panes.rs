//! Splitting one terminal into several panes, the way Termoil splits a tab (`task-2597`).
//!
//! A terminal in Unluminous belongs to something: a tab in the terminal tile, a run, a node on the canvas, a
//! ticket's agent. That owner holds one session and knows nothing about splits, so a split is described here
//! as a layout of pane numbers, and pane `0` is always the owner's own session. The window keeps the other
//! panes' sessions beside the layout. Nothing here starts a program or draws anything, so all of it is plain
//! values with plain expected answers.
//!
//! Positions are fractions of the area the terminal is given, from `0.0` to `1.0`, so the same layout is
//! laid out again at whatever size the area is next frame.

/// A pane's number. `0` is the owner's own terminal and is never closed by this layout.
pub type PaneId = u32;

/// The owner's own pane.
pub const HOST: PaneId = 0;

/// The smallest share of a split either side keeps when a divider is dragged or moved.
pub const SMALLEST_SHARE: f32 = 0.1;

/// How far one press of the resize keys moves a divider, as a share of the split.
pub const RESIZE_STEP: f32 = 0.05;

/// A side of a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// The direction with this name: `left`, `right`, `up` or `down`, ignoring case.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            _ => None,
        }
    }

    /// The direction's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    /// True for left and right, whose panes sit side by side.
    fn side_by_side(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// A rectangle as fractions of the area: left, top, width and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frac {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Frac {
    /// The whole area.
    pub const WHOLE: Frac = Frac { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// A divider between the two halves of a split, for drawing it and for dragging it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Divider {
    /// Which split it is, counted the way [`Layout::set_share`] counts them.
    pub split: usize,
    /// True when the halves sit side by side, so the divider is an upright line.
    pub side_by_side: bool,
    /// Where the divider is: `x` and `y` of its line, and its length, as fractions of the area.
    pub at: Frac,
    /// The area the split itself fills, which a drag measures the new share against.
    pub within: Frac,
}

/// One node of the layout.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Pane(PaneId),
    Split { side_by_side: bool, share: f32, first: Box<Node>, second: Box<Node> },
}

impl Node {
    fn panes(&self, out: &mut Vec<PaneId>) {
        match self {
            Node::Pane(id) => out.push(*id),
            Node::Split { first, second, .. } => {
                first.panes(out);
                second.panes(out);
            }
        }
    }

    fn lay_out(&self, area: Frac, out: &mut Vec<(PaneId, Frac)>, dividers: &mut Vec<Divider>) {
        match self {
            Node::Pane(id) => out.push((*id, area)),
            Node::Split { side_by_side, share, first, second } => {
                let (a, b) = halves(area, *side_by_side, *share);
                let at = match side_by_side {
                    true => Frac { x: b.x, y: area.y, w: 0.0, h: area.h },
                    false => Frac { x: area.x, y: b.y, w: area.w, h: 0.0 },
                };
                dividers.push(Divider {
                    split: dividers.len(),
                    side_by_side: *side_by_side,
                    at,
                    within: area,
                });
                first.lay_out(a, out, dividers);
                second.lay_out(b, out, dividers);
            }
        }
    }

    /// Replace the pane `target` with a split of it and `added`, `added` on side `direction`.
    fn split(&mut self, target: PaneId, added: PaneId, direction: Direction) -> bool {
        match self {
            Node::Pane(id) if *id == target => {
                let old = Box::new(Node::Pane(target));
                let new = Box::new(Node::Pane(added));
                let (first, second) = match direction {
                    Direction::Right | Direction::Down => (old, new),
                    Direction::Left | Direction::Up => (new, old),
                };
                *self = Node::Split {
                    side_by_side: direction.side_by_side(),
                    share: 0.5,
                    first,
                    second,
                };
                true
            }
            Node::Pane(_) => false,
            Node::Split { first, second, .. } => {
                first.split(target, added, direction) || second.split(target, added, direction)
            }
        }
    }

    /// Take the pane `target` out, letting its sibling have the split's place. Answers the pane that should
    /// have the keyboard afterwards: the first pane of the sibling.
    fn remove(&mut self, target: PaneId) -> Option<PaneId> {
        let Node::Split { first, second, .. } = self else { return None };
        let keep = if **first == Node::Pane(target) {
            Some(std::mem::replace(second.as_mut(), Node::Pane(HOST)))
        } else if **second == Node::Pane(target) {
            Some(std::mem::replace(first.as_mut(), Node::Pane(HOST)))
        } else {
            None
        };
        if let Some(kept) = keep {
            let mut panes = Vec::new();
            kept.panes(&mut panes);
            *self = kept;
            return panes.first().copied();
        }
        let Node::Split { first, second, .. } = self else { return None };
        first.remove(target).or_else(|| second.remove(target))
    }

    /// The splits from the root down to the pane `target`, as `(split number, the pane is in the first half)`.
    fn path_to(
        &self,
        target: PaneId,
        counter: &mut usize,
        out: &mut Vec<(usize, bool, bool)>,
    ) -> bool {
        match self {
            Node::Pane(id) => *id == target,
            Node::Split { side_by_side, first, second, .. } => {
                let number = *counter;
                *counter += 1;
                out.push((number, *side_by_side, true));
                if first.path_to(target, counter, out) {
                    return true;
                }
                out.pop();
                out.push((number, *side_by_side, false));
                if second.path_to(target, counter, out) {
                    return true;
                }
                out.pop();
                false
            }
        }
    }

    /// The split numbered `wanted`, counting in the order [`Node::lay_out`] meets them.
    fn nth_split(&mut self, wanted: usize, counter: &mut usize) -> Option<&mut f32> {
        let Node::Split { share, first, second, .. } = self else { return None };
        if *counter == wanted {
            return Some(share);
        }
        *counter += 1;
        if let Some(found) = first.nth_split(wanted, counter) {
            return Some(found);
        }
        second.nth_split(wanted, counter)
    }
}

/// The two halves of `area`, split side by side or one above the other, the first taking `share` of it.
fn halves(area: Frac, side_by_side: bool, share: f32) -> (Frac, Frac) {
    match side_by_side {
        true => {
            let w = area.w * share;
            (Frac { w, ..area }, Frac { x: area.x + w, w: area.w - w, ..area })
        }
        false => {
            let h = area.h * share;
            (Frac { h, ..area }, Frac { y: area.y + h, h: area.h - h, ..area })
        }
    }
}

/// How one terminal is split: the panes, which has the keyboard, and whether one fills the area alone.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    root: Node,
    focused: PaneId,
    zoomed: bool,
    next: PaneId,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    /// One pane, the owner's.
    pub fn new() -> Self {
        Self { root: Node::Pane(HOST), focused: HOST, zoomed: false, next: HOST + 1 }
    }

    /// Every pane, in reading order: left before right and top before bottom.
    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.root.panes(&mut out);
        out
    }

    /// True when there is more than one pane.
    pub fn is_split(&self) -> bool {
        matches!(self.root, Node::Split { .. })
    }

    /// The pane that has the keyboard.
    pub fn focused(&self) -> PaneId {
        self.focused
    }

    /// Give the keyboard to a pane. False when there is no such pane.
    pub fn focus(&mut self, pane: PaneId) -> bool {
        if !self.panes().contains(&pane) {
            return false;
        }
        self.focused = pane;
        true
    }

    /// True while the focused pane fills the area alone.
    pub fn is_zoomed(&self) -> bool {
        self.zoomed
    }

    /// Show the focused pane alone, or put the split back: Termoil's `zoom-pane`. Answers whether it is now
    /// zoomed. A layout with one pane has nothing to zoom.
    pub fn toggle_zoom(&mut self) -> bool {
        self.zoomed = !self.zoomed && self.is_split();
        self.zoomed
    }

    /// Split the focused pane, the new pane going on side `direction`, and give the new pane the keyboard.
    /// Answers the new pane's number, which the caller starts a session for.
    pub fn split(&mut self, direction: Direction) -> PaneId {
        let added = self.next;
        self.next += 1;
        self.root.split(self.focused, added, direction);
        self.focused = added;
        self.zoomed = false;
        added
    }

    /// Take a pane out. The owner's pane is never taken out, and false is the answer for it and for a pane
    /// that is not there. The keyboard goes to the pane that took its place.
    pub fn close(&mut self, pane: PaneId) -> bool {
        if pane == HOST || !self.panes().contains(&pane) {
            return false;
        }
        let heir = self.root.remove(pane);
        if self.focused == pane || !self.panes().contains(&self.focused) {
            self.focused = heir.unwrap_or(HOST);
        }
        if !self.is_split() {
            self.zoomed = false;
        }
        true
    }

    /// Where each pane goes in `area`, and where each divider is. A zoomed layout is the focused pane alone
    /// with no dividers.
    pub fn lay_out(&self, area: Frac) -> (Vec<(PaneId, Frac)>, Vec<Divider>) {
        if self.zoomed {
            return (vec![(self.focused, area)], Vec::new());
        }
        let mut panes = Vec::new();
        let mut dividers = Vec::new();
        self.root.lay_out(area, &mut panes, &mut dividers);
        (panes, dividers)
    }

    /// The pane next to the focused one on side `direction`: of the panes whose edge touches that side and
    /// which overlap it across, the one nearest its middle. `None` at the edge of the area.
    pub fn neighbor(&self, direction: Direction) -> Option<PaneId> {
        let (panes, _) = self.lay_out_unzoomed();
        let (_, here) = *panes.iter().find(|(id, _)| *id == self.focused)?;
        let (cx, cy) = here.center();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        panes
            .iter()
            .filter(|(id, _)| *id != self.focused)
            .filter(|(_, there)| match direction {
                Direction::Left => close(there.x + there.w, here.x),
                Direction::Right => close(there.x, here.x + here.w),
                Direction::Up => close(there.y + there.h, here.y),
                Direction::Down => close(there.y, here.y + here.h),
            })
            .filter(|(_, there)| match direction.side_by_side() {
                true => there.y < here.y + here.h - 1e-4 && there.y + there.h > here.y + 1e-4,
                false => there.x < here.x + here.w - 1e-4 && there.x + there.w > here.x + 1e-4,
            })
            .min_by(|(_, a), (_, b)| {
                let distance = |f: &Frac| {
                    let (x, y) = f.center();
                    (x - cx).abs() + (y - cy).abs()
                };
                distance(a).total_cmp(&distance(b))
            })
            .map(|(id, _)| *id)
    }

    /// Give the keyboard to the pane next to the focused one. False when there is none on that side.
    pub fn focus_toward(&mut self, direction: Direction) -> bool {
        match self.neighbor(direction) {
            Some(pane) => {
                self.focused = pane;
                true
            }
            None => false,
        }
    }

    /// The next pane in reading order after the focused one, wrapping round, or the one before it.
    pub fn cycle(&mut self, forward: bool) -> PaneId {
        let panes = self.panes();
        let at = panes.iter().position(|id| *id == self.focused).unwrap_or(0);
        let next = match forward {
            true => (at + 1) % panes.len(),
            false => (at + panes.len() - 1) % panes.len(),
        };
        self.focused = panes[next];
        self.focused
    }

    /// Move the divider nearest the focused pane along the axis of `direction` one step that way, which
    /// grows the pane on one side of it and shrinks the other: Termoil's `resize-split`. False when no
    /// divider lies along that axis.
    pub fn resize(&mut self, direction: Direction) -> bool {
        let mut path = Vec::new();
        self.root.path_to(self.focused, &mut 0, &mut path);
        let wanted = direction.side_by_side();
        let Some(&(split, _, _)) =
            path.iter().rev().find(|(_, side_by_side, _)| *side_by_side == wanted)
        else {
            return false;
        };
        let Some(share) = self.root.nth_split(split, &mut 0) else { return false };
        let moved = match direction {
            Direction::Right | Direction::Down => *share + RESIZE_STEP,
            Direction::Left | Direction::Up => *share - RESIZE_STEP,
        };
        *share = moved.clamp(SMALLEST_SHARE, 1.0 - SMALLEST_SHARE);
        true
    }

    /// Set the share of the first half of split `split`, which is what dragging its divider does.
    pub fn set_share(&mut self, split: usize, share: f32) -> bool {
        match self.root.nth_split(split, &mut 0) {
            Some(held) => {
                *held = share.clamp(SMALLEST_SHARE, 1.0 - SMALLEST_SHARE);
                true
            }
            None => false,
        }
    }

    fn lay_out_unzoomed(&self) -> (Vec<(PaneId, Frac)>, Vec<Divider>) {
        let mut panes = Vec::new();
        let mut dividers = Vec::new();
        self.root.lay_out(Frac::WHOLE, &mut panes, &mut dividers);
        (panes, dividers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_layout_is_the_owners_pane_alone() {
        let layout = Layout::new();
        assert_eq!(layout.panes(), vec![HOST]);
        assert!(!layout.is_split());
        let (panes, dividers) = layout.lay_out(Frac::WHOLE);
        assert_eq!(panes, vec![(HOST, Frac::WHOLE)]);
        assert!(dividers.is_empty());
    }

    #[test]
    fn splitting_right_then_down_makes_three_panes_in_reading_order() {
        let mut layout = Layout::new();
        let right = layout.split(Direction::Right);
        assert_eq!(layout.focused(), right);
        let below = layout.split(Direction::Down);
        assert_eq!(layout.panes(), vec![HOST, right, below]);
        let (panes, dividers) = layout.lay_out(Frac::WHOLE);
        assert_eq!(panes[0].1, Frac { x: 0.0, y: 0.0, w: 0.5, h: 1.0 });
        assert_eq!(panes[1].1, Frac { x: 0.5, y: 0.0, w: 0.5, h: 0.5 });
        assert_eq!(panes[2].1, Frac { x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
        assert_eq!(dividers.len(), 2);
        assert!(dividers[0].side_by_side && !dividers[1].side_by_side);
    }

    #[test]
    fn splitting_left_puts_the_new_pane_first() {
        let mut layout = Layout::new();
        let added = layout.split(Direction::Left);
        assert_eq!(layout.panes(), vec![added, HOST]);
    }

    #[test]
    fn focus_moves_to_the_neighbor_on_each_side_and_stops_at_the_edge() {
        let mut layout = Layout::new();
        let right = layout.split(Direction::Right);
        let below = layout.split(Direction::Down);
        assert!(layout.focus_toward(Direction::Up));
        assert_eq!(layout.focused(), right);
        assert!(layout.focus_toward(Direction::Left));
        assert_eq!(layout.focused(), HOST);
        assert!(!layout.focus_toward(Direction::Left), "nothing left of the first pane");
        assert!(layout.focus_toward(Direction::Right));
        assert_eq!(layout.focused(), right, "the nearer of the two on the right");
        assert_eq!(layout.cycle(true), below);
        assert_eq!(layout.cycle(true), HOST, "wraps round");
    }

    #[test]
    fn closing_a_pane_gives_its_place_and_the_keyboard_to_its_sibling() {
        let mut layout = Layout::new();
        let right = layout.split(Direction::Right);
        let below = layout.split(Direction::Down);
        assert!(layout.close(below));
        assert_eq!(layout.panes(), vec![HOST, right]);
        assert_eq!(layout.focused(), right);
        assert!(!layout.close(HOST), "the owner's pane stays");
        assert!(layout.close(right));
        assert!(!layout.is_split());
        assert_eq!(layout.focused(), HOST);
        assert!(!layout.close(right), "already gone");
    }

    #[test]
    fn zoom_shows_the_focused_pane_alone_and_a_split_ends_it() {
        let mut layout = Layout::new();
        assert!(!layout.toggle_zoom(), "one pane has nothing to zoom");
        let right = layout.split(Direction::Right);
        assert!(layout.toggle_zoom());
        let (panes, dividers) = layout.lay_out(Frac::WHOLE);
        assert_eq!(panes, vec![(right, Frac::WHOLE)]);
        assert!(dividers.is_empty());
        layout.split(Direction::Down);
        assert!(!layout.is_zoomed());
    }

    #[test]
    fn resize_moves_the_nearest_divider_and_keeps_both_sides_visible() {
        let mut layout = Layout::new();
        layout.split(Direction::Right);
        layout.focus(HOST);
        assert!(layout.resize(Direction::Right));
        let (panes, _) = layout.lay_out(Frac::WHOLE);
        assert!((panes[0].1.w - 0.55).abs() < 1e-4);
        assert!(!layout.resize(Direction::Down), "no split one above the other");
        for _ in 0..40 {
            layout.resize(Direction::Right);
        }
        let (panes, _) = layout.lay_out(Frac::WHOLE);
        assert!((panes[0].1.w - (1.0 - SMALLEST_SHARE)).abs() < 1e-4);
        assert!(layout.set_share(0, 0.0));
        let (panes, _) = layout.lay_out(Frac::WHOLE);
        assert!((panes[0].1.w - SMALLEST_SHARE).abs() < 1e-4);
    }

    #[test]
    fn directions_are_read_by_name() {
        for direction in [Direction::Left, Direction::Right, Direction::Up, Direction::Down] {
            assert_eq!(Direction::parse(direction.name()), Some(direction));
        }
        assert_eq!(Direction::parse(" Right "), Some(Direction::Right));
        assert_eq!(Direction::parse("sideways"), None);
    }
}
