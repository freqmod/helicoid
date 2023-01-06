use std::{collections::HashMap, sync::Arc};

//use crate::text::ShapedTextBlock;
use bytecheck::CheckBytes;
use num_enum::IntoPrimitive;
use ordered_float::OrderedFloat;
use parking_lot::Mutex;
use rkyv::{Archive, Deserialize, Serialize};
use smallvec::SmallVec;
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum HelicoidToServerMessage {
    ViewportSizeUpdate(ViewportInfo),
    KeyModifierStateUpdate(KeyModifierStateUpdateEvent),
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
    physical_size: (u32, u32),
    scale_factor: u32,
    container_physical_size: Option<(u32, u32)>,
    container_scale_factor: Option<u32>,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct SimpleKeyTappedEvent {
    key_code: u32, /* Virutual key code, as represented in winit */
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct KeyModifierStateUpdateEvent {
    shift_pressed: bool,
    ctrl_pressed: bool,
    alt_pressed: bool,
    logo_pressed: bool,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct MouseButtonStateChangeEvent {
    pressed: bool,
    button: u16,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct CursorMovedEvent {
    physical_position_x: OrderedFloat<f32>,
    physical_position_y: OrderedFloat<f32>,
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
