use window::create_window;

mod bridge;
mod channel_utils;
mod editor;
mod event_aggregator;
mod frame;
mod redraw_scheduler;
mod renderer;
mod window;
mod dimensions;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate lazy_static;

fn main() {
    create_window();
}
