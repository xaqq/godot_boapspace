use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = Resource)]
pub(crate) struct SelectedTile {
    cell: Option<(i32, i32, GString)>,

    base: Base<Resource>,
}

#[godot_api]
impl IResource for SelectedTile {
    fn init(base: Base<Resource>) -> Self {
        Self {
            cell: None,
            base,
        }
    }
}

#[godot_api]
impl SelectedTile {
    pub(crate) fn select(&mut self, x: i32, y: i32, name: GString) {
        self.cell = Some((x, y, name));
    }

    pub(crate) fn deselect(&mut self) {
        self.cell = None;
    }

    pub(crate) fn cell_x(&self) -> Option<i32> {
        self.cell.as_ref().map(|(x, _, _)| *x)
    }

    pub(crate) fn cell_y(&self) -> Option<i32> {
        self.cell.as_ref().map(|(_, y, _)| *y)
    }

    pub(crate) fn type_name(&self) -> Option<GString> {
        self.cell.as_ref().map(|(_, _, n)| n.clone())
    }
}
