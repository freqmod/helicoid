use window::create_window;

mod bridge;
mod channel_utils;
mod dimensions;
mod editor;
mod event_aggregator;
mod frame;
mod redraw_scheduler;
mod renderer;
mod window;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate lazy_static;

fn main() {
    env_logger::init();
    create_window();
}
