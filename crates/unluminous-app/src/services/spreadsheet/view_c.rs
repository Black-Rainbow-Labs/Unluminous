//! What a tab remembers about the objects on its sheet (`task-2360`, the `wiringc` lane): which are chosen,
//! the right click menu of an object, and the kind of shape the next drag on the grid draws. It is one field of
//! the view so the view's own file stays small.

use unluminous_sheet::objects::ObjectId;

/// The chosen objects, the object menu and the shape being drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectState {
    /// The objects chosen, in the order they were chosen. The first is the one the Format level shows.
    pub selected: Vec<ObjectId>,
    /// Where the object menu was opened, when it is open.
    pub menu: Option<egui::Pos2>,
    /// A shape kind word (`rectangle`, `arrow`, `textbox`) while the next drag on the grid draws one.
    pub drawing: Option<String>,
    /// Where each object was last drawn in the window, left, top, width and height in the window's own
    /// points, so a pointer driven from outside can find it. The last pane drawn wins.
    pub shown: Vec<(ObjectId, [f32; 4])>,
}

impl ObjectState {
    /// Choose one object only, or nothing.
    pub fn choose(&mut self, id: Option<ObjectId>) {
        self.selected = id.into_iter().collect();
    }

    /// Add an object to the choice, or take it out when it is already there.
    pub fn toggle(&mut self, id: ObjectId) {
        match self.selected.iter().position(|s| *s == id) {
            Some(at) => {
                self.selected.remove(at);
            }
            None => self.selected.push(id),
        }
    }

    /// Whether an object is chosen.
    pub fn has(&self, id: ObjectId) -> bool {
        self.selected.contains(&id)
    }
}
