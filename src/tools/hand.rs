//! The default pan/zoom tool. Doesn't author any edits — pan + zoom are handled
//! by the scroll view itself, so this tool's job is just to be the inert
//! "no-op" mode the user is in when they're reading rather than editing.

use super::Tool;

#[derive(Default)]
pub struct HandTool;

impl Tool for HandTool {
    fn id(&self) -> &'static str {
        "hand"
    }

    fn label(&self) -> &'static str {
        "Hand"
    }
}
