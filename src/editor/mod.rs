use std::{collections::HashMap, sync::Arc, thread};

use log::{error, trace};

use crate::{
    bridge::{GuiOption, RedrawEvent, WindowAnchor},
    event_aggregator::EVENT_AGGREGATOR,
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::DrawCommand,
    window::WindowCommand,
};

const MODE_CMDLINE: u64 = 4;

mod cursor;
mod style;

pub use cursor::{Cursor, CursorMode, CursorShape};
pub use style::{Colors, Style, UnderlineStyle};

#[derive(Clone, Debug)]
pub struct AnchorInfo {
    pub anchor_grid_id: u64,
    pub anchor_type: WindowAnchor,
    pub anchor_left: f64,
    pub anchor_top: f64,
    pub sort_order: u64,
}

impl WindowAnchor {
    fn modified_top_left(
        &self,
        grid_left: f64,
        grid_top: f64,
        width: u64,
        height: u64,
    ) -> (f64, f64) {
        match self {
            WindowAnchor::NorthWest => (grid_left, grid_top),
            WindowAnchor::NorthEast => (grid_left - width as f64, grid_top),
            WindowAnchor::SouthWest => (grid_left, grid_top - height as f64),
            WindowAnchor::SouthEast => (grid_left - width as f64, grid_top - height as f64),
        }
    }
}

#[derive(Clone, Debug)]
pub enum EditorCommand {
    NeovimRedrawEvent(RedrawEvent),
    RedrawScreen,
}
