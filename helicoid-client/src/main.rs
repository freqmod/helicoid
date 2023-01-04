use clap::Parser;
use command_line::HeliconeCommandLineArguments;
use window::create_window;

mod bridge;
mod channel_utils;
mod command_line;
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
/// Simple program to greet a person

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = HeliconeCommandLineArguments::parse();
    create_window(&args);
}
