mod tcp_bridge;
pub struct BridgeMessage {}

/* This is the start of an (currently imaginary) bridge to the helix editor.
This bridge is having a quite different architecture than the (neo)vim bridge. */

#[derive(Clone, Debug)]
pub enum WindowAnchor {
    NorthWest,
    NorthEast,
    SouthWest,
    SouthEast,
}

#[derive(Clone, Debug)]
pub enum RedrawEvent {}

#[derive(Clone, Debug)]
pub enum GuiOption {
    ArabicShape(bool),
    AmbiWidth(String),
    Emoji(bool),
    GuiFont(String),
    GuiFontSet(String),
    GuiFontWide(String),
    LineSpace(u64),
    Pumblend(u64),
    ShowTabLine(u64),
    TermGuiColors(bool),
    //Unknown(String, ),
}
