#[macro_use]
extern crate lazy_static;

use anyhow::{Result};
use clap::Parser;
use futures::StreamExt;
use server::HelicoidServer;
use std::future;


use termion::{event::Key, raw::IntoRawMode};
use termion_input_tokio::TermReadAsync;
use tokio::runtime::Runtime;
use tokio::time as ttime;
//use futures_util::stream::stream::StreamExt;

mod center;
mod compositor;
mod constants;
mod editor;
mod editor_view;
mod server;
mod statusline;

#[cfg(test)]
mod tests;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CommandLineArguments {
    /// Listening address of editor server, for connecting to an existing server
    #[arg(short, long, default_value = "127.0.0.1:15566")]
    pub server_address: String,
    /*    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,*/
}

fn main() -> Result<()> {
    env_logger::init();
    let args = CommandLineArguments::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .thread_stack_size(16 * 1024 * 1024) // Especially in debug mode the font shaping stuff may need some more stack
        .enable_io()
        .build()?;
    runtime.spawn(async move {
        let mut bridge_server = HelicoidServer::new(args.server_address).await.unwrap();
        bridge_server.event_loop().await.unwrap();
    });
    wait_for_input();
    /* Wait for server to end */
    Ok(())
}

fn wait_for_input() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async move {
        // Disable line buffering, local echo, etc.
        let raw_term = std::io::stdout().into_raw_mode();
        if raw_term.is_err() {
            println!("Failed to acquire raw access to the terminal, sleeping forever instead");
            loop {
                ttime::sleep(std::time::Duration::from_secs(1)).await;
            }
            return;
        }
        log::warn!("Press q to quit");

        tokio::io::stdin()
            .keys_stream()
            // End the stream when 'q' is pressed.
            .take_while(|event| {
                future::ready(match event {
                    Ok(Key::Char('q')) => false,
                    _ => true,
                })
            })
            // Print each key that was pressed.
            .for_each(|event| async move {
                log::trace!("{:?}\r", event);
            })
            .await;
    });
}
