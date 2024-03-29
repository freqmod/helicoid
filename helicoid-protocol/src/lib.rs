pub mod block_manager;
pub mod bridge_logic;
pub mod caching_shaper;
pub mod font_options;
pub mod gfx;
pub mod input;
pub mod shadowblocks;
pub mod swash_font;
#[cfg(feature = "tokio")]
pub mod tcp_bridge_async;
pub mod tcp_bridge_sync;
pub mod text;
pub mod transferbuffer;

#[macro_use]
extern crate derive_new;

#[cfg(test)]
mod tests {

    #[test]
    fn it_works() {}
}
