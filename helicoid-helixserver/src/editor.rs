use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::{
    broadcast::{self, Receiver as BReceiver, Sender as BSender},
    mpsc::{self, Receiver, Sender},
    Mutex as TMutex,
};

use helix_core::{config::user_syntax_loader, syntax};
use helix_view::{editor::Config, graphics::Rect, theme, Editor as VEditor};

/* Architecture:
The (Dummy)Editor object is stored in a shared Arc<TMutex<>> object, and is cloned
to all the client handles. All clients register with the editor to be notified (using a channel)
when there are changes. When the editing model has changed they will determine if the client
needs an update. */
pub struct Editor {
    editor_state_changed_send: tokio::sync::broadcast::Sender<()>,
    editor: VEditor,
}

impl Editor {
    pub fn new() -> Self {
        let (editor_state_changed_send, _) = broadcast::channel(1);
        let exe_path = std::env::current_exe().unwrap();
        let resource_path = exe_path
            .as_path()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let paths = vec![resource_path.join("user"), resource_path.join("default")];
        let theme_loader = Arc::new(theme::Loader::new(&paths));
        let syn_loader_conf = user_syntax_loader().unwrap();
        let syn_loader = std::sync::Arc::new(syntax::Loader::new(syn_loader_conf));
        //        let syn_loader = Arc::new(helix_core::syntax::Loader::new());
        /*        let config = match std::fs::read_to_string(config_dir.join("config.toml")) {
            Ok(config) => toml::from_str(&config)
                //                .map(helix_term::keymap::merge_keys)
                .unwrap_or_else(|err| {
                    eprintln!("Bad config: {}", err);
                    eprintln!("Press <ENTER> to continue with default config");
                    use std::io::Read;
                    // This waits for an enter press.
                    let _ = std::io::stdin().read(&mut []);
                    Config::default()
                }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(err) => return Err(Error::new(err)),
        };*/
        let config = Arc::new(ArcSwap::from_pointee(Config::default()));
        Self {
            editor_state_changed_send,
            editor: VEditor::new(Rect::new(0, 0, 10, 10), theme_loader, syn_loader, config),
            //            text: String::new(),
        }
    }
    pub fn update_receiver(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.editor_state_changed_send.subscribe()
    }
    pub fn editor_mut(&mut self) -> &mut VEditor {
        &mut self.editor
    }
    pub fn editor(&self) -> &VEditor {
        &self.editor
    }
}
