use clap::Parser;
use command_line::HeliconeCommandLineArguments;
use window::create_window;

mod bridge;
//mod channel_utils;
mod command_line;
mod dimensions;
mod editor;
//mod event_aggregator;
mod frame;
mod redraw_scheduler;
mod renderer;
mod window;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate lazy_static;
/// Simple program to greet a person

//#[tokio::main]
fn main() {
    env_logger::init();
    let args = HeliconeCommandLineArguments::parse();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .thread_stack_size(16 * 1024 * 1024) // Especially in debug mode the font shaping stuff may need some more stack
        .enable_io()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    /* We still want to run the GUI on the main thread, as this has benefits on some OSes */
    create_window(&args);
}
