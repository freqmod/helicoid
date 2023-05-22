

//use crate::text::ShapedTextBlock;
use bytecheck::CheckBytes;
use num_enum::IntoPrimitive;
use ordered_float::OrderedFloat;

use rkyv::{Archive, Deserialize, Serialize};
use smallvec::SmallVec;
/* The timestamp is a u32, and contains a relative timestamp of the event in ms
(as a wrapping u32). There is no definition of a start time, the events are
relative to eachother in a session. */
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum HelicoidToServerMessage {
    ViewportSizeUpdate(ViewportInfo),
    KeyModifierStateUpdate(KeyModifierStateUpdateEvent),
    KeyInputEvent(ComplexKeyEvent),
    KeyPressedEvent(SimpleKeyTappedEvent),
    MouseButtonStateChange(MouseButtonStateChangeEvent),
    CursorMoved(CursorMovedEvent),
    CharReceived(u32),
    Ime(ImeEvent),
    /* Answer a request (from the editor server) for system keyboard contents.
    Currently the answer is limited to 15kb (to fit the TCPBridge without any fuzz) */
    ClipboardEvent(String),
    /* It is probably desirable to report more detailed keyboard movement at a later point to
    enable as much keyboard control as possible */
    //    ExtendedKeyEvent(ExtendedKeyEvent),
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct ViewportInfo {
    pub physical_size: (u32, u32),
    pub scale_factor: OrderedFloat<f32>,
    pub container_physical_size: Option<(u32, u32)>,
    pub container_scale_factor: Option<u32>,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct SimpleKeyTappedEvent {
    pub key_code: u32, /* Virutual key code, as represented in winit */
    pub timestamp: u32,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct ComplexKeyEvent {
    pub key_code: u32, /* Virutual key code, as represented in winit */
    pub timestamp: u32,
    pub virtual_keycode: VirtualKeycode,
    pub pressed: bool, /* True if key was pressed, false if it was released */
    pub synthetic: bool,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct KeyModifierStateUpdateEvent {
    pub caps_pressed: bool,
    pub lshift_pressed: bool,
    pub lctrl_pressed: bool,
    pub lalt_pressed: bool,
    pub llogo_pressed: bool,
    /* Currently right side is not supported by winit, but add them so they are here when they do */
    pub rshift_pressed: bool,
    pub rctrl_pressed: bool,
    pub ralt_pressed: bool,
    pub rlogo_pressed: bool,
    pub bits: u32,
    pub timestamp: u32,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct MouseButtonStateChangeEvent {
    pub pressed: bool,
    pub button: u16,
    pub timestamp: u32,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct CursorMovedEvent {
    pub physical_position_x: OrderedFloat<f32>,
    pub physical_position_y: OrderedFloat<f32>,
    pub timestamp: u32,
}
/* See winit Ime event for details, expects the strings in the smallvec to
be utf-8 encoded, and relatively short (max len 255 bytes) */
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum ImeEvent {
    Enabled,
    Preedit((SmallVec<[u8; 20]>, Option<(u8, u8)>)),
    Commit(SmallVec<[u8; 20]>),
    Disabled,
}

/* Based on winit 0.28 */
#[derive(
    Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
#[derive(IntoPrimitive)]
#[repr(u16)]
pub enum VirtualKeycode {
    #[default]
    None,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,

    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    Escape,

    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    Snapshot,
    Scroll,
    Pause,

    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,

    Left,
    Up,
    Right,
    Down,

    Backspace,
    Return,
    Space,

    Compose,

    Caret,

    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadDivide,
    NumpadDecimal,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    NumpadMultiply,
    NumpadSubtract,

    AbntC1,
    AbntC2,
    Apostrophe,
    Apps,
    Asterisk,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Mute,
    MyComputer,
    NavigateForward,  // or next
    NavigateBackward, // or prior
    NextTrack,
    NoConvert,
    OEM102,
    Period,
    PlayPause,
    Plus,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Copy,
    Paste,
    Cut,
}
