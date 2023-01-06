use clap::Parser;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct HeliconeCommandLineArguments {
    /// Address of editor server, for connecting to an existing server
    #[arg(short, long, default_value = "127.0.0.1:15566")]
    pub server_address: Option<String>,
    /*    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,*/
}
