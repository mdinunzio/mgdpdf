//! The `Tool` trait, event types, and a registry of available tools.
//!
//! Every editing feature plugs in by implementing [`Tool`]. The `page_view`
//! widget feeds each visible page's input to the active tool and gives it a
//! painter for an overlay. Tools mutate the [`EditSession`] via the
//! [`UndoStack`] inside [`ToolCtx`] so every change is undoable.
//!
//! Scaffolding warnings (unused fields and trait methods) are silenced for
//! the whole module — they become live once Phases 3+ register real tools.
#![allow(dead_code)]

pub mod form_fill;
pub mod hand;

use egui::{Painter, Pos2, Sense, Ui};

use crate::edit::{command::Command, EditSession, UndoStack};
use crate::pdf::document::TextFieldWidget;
use crate::pdf::PageTransform;

pub use form_fill::FormFillTool;
pub use hand::HandTool;

/// One-page-scoped input event delivered to the active tool.
#[derive(Copy, Clone, Debug)]
pub enum ToolEvent {
    /// The pointer moved while over this page. `pdf` is in PDF point coords.
    PointerMove { pdf: Pos2 },
    /// Primary mouse button pressed at `pdf` (PDF coords).
    PointerDown { pdf: Pos2 },
    /// Primary mouse button released at `pdf` (PDF coords).
    PointerUp { pdf: Pos2 },
    /// Pointer left the page rect.
    PointerLeave,
}

/// Mutable bundle of state a tool may need. Held by `&mut` for the duration of
/// a single event/draw call — never stored.
pub struct ToolCtx<'a> {
    pub session: &'a mut EditSession,
    pub undo: &'a mut UndoStack,
    /// All text-field widgets in the open document, computed once on open.
    pub widgets: &'a [TextFieldWidget],
}

impl ToolCtx<'_> {
    /// Pushes a reversible command through the undo stack and applies it.
    pub fn run<C: Command>(&mut self, command: C) {
        self.undo.push_apply(Box::new(command), self.session);
    }
}

/// A user-selectable editing mode (pan/zoom, form fill, free text, etc.).
pub trait Tool {
    /// Stable identifier used by [`ToolBox`] and persisted in settings.
    fn id(&self) -> &'static str;

    /// Short label shown in the toolbar.
    fn label(&self) -> &'static str;

    /// Receives a single page-scoped event. The default does nothing — most
    /// tools only care about a subset.
    fn on_event(&mut self, _page_index: usize, _event: ToolEvent, _ctx: &mut ToolCtx<'_>) {}

    /// What kind of input the page background should accept while this tool
    /// is active. Default is `hover` only — so click-drag passes through to
    /// the surrounding `ScrollArea` and the user can pan/scroll. Tools that
    /// need to detect clicks or drags on the bare page (free-text placement,
    /// highlight selection, signature stamp) override to `click_and_drag`.
    fn page_sense(&self) -> Sense {
        Sense::hover()
    }

    /// Draws the tool's non-interactive overlay for one visible page. Called
    /// after the page bitmap is painted, with `painter` clipped to the page's
    /// screen rect. The default draws nothing.
    fn draw_overlay(
        &self,
        _page_index: usize,
        _painter: &Painter,
        _transform: &PageTransform,
        _session: &EditSession,
    ) {
    }

    /// Like [`draw_overlay`] but allowed to add interactive widgets (text
    /// inputs, drag handles, etc.) via `ui`. Called per visible page after
    /// `draw_overlay`. `ui` is scoped to the page's screen rect.
    fn draw_interactive(
        &mut self,
        _page_index: usize,
        _ui: &mut Ui,
        _transform: &PageTransform,
        _ctx: &mut ToolCtx<'_>,
    ) {
    }
}

/// Registry of available tools plus the active selection. Owned by `App`.
pub struct ToolBox {
    tools: Vec<Box<dyn Tool>>,
    active: usize,
}

impl Default for ToolBox {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolBox {
    pub fn new() -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(HandTool::default()),
            Box::new(FormFillTool::default()),
        ];
        Self { tools, active: 0 }
    }

    /// Registers an additional tool. Returns its index.
    #[allow(dead_code)] // Used by Phases 3+.
    pub fn register(&mut self, tool: Box<dyn Tool>) -> usize {
        self.tools.push(tool);
        self.tools.len() - 1
    }

    pub fn tools(&self) -> impl Iterator<Item = (usize, &dyn Tool)> {
        self.tools.iter().enumerate().map(|(i, t)| (i, t.as_ref()))
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tools.len() {
            self.active = index;
        }
    }

    pub fn active(&self) -> &dyn Tool {
        self.tools[self.active].as_ref()
    }

    pub fn active_mut(&mut self) -> &mut dyn Tool {
        self.tools[self.active].as_mut()
    }
}
