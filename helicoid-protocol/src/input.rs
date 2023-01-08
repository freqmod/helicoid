use std::{collections::HashMap, sync::Arc};

//use crate::text::ShapedTextBlock;
use bytecheck::CheckBytes;
use num_enum::IntoPrimitive;
use ordered_float::OrderedFloat;
use parking_lot::Mutex;
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
pub struct KeyModifierStateUpdateEvent {
    pub shift_pressed: bool,
    pub ctrl_pressed: bool,
    pub alt_pressed: bool,
    pub logo_pressed: bool,
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
