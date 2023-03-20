use helicoid_protocol::{
    dataflow::ShadowMetaContainerBlock,
    gfx::{PointF16, PointF32, PointU32, RenderBlockId},
};
use helix_view::{document::Mode, Editor};
use ordered_float::OrderedFloat;
use std::hash::Hash;
use swash::Metrics;

trait GfxComposibleBlock: Hash + PartialEq {
    fn extent(&self) -> PointU32;
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32);
    fn render(&mut self);
}
#[derive(Hash, PartialEq, Clone)]
struct SizeScale {
    line_height: OrderedFloat<f32>,
}
#[derive(Hash, PartialEq, Clone)]
struct ShadowMetaBlock {}
/* Top at the moment is not in use */
#[derive(Hash, PartialEq)]
struct EditorTop {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}

/* How should we organise the status line, helix view has a very string based approach
while it would be nice with a bit more semantics here to enable more fancy graphics
(e.g. for file edited state) */
#[derive(Hash, PartialEq)]
struct StatusLineModel {
    //    status_mode: Mode,
    //    doucment_name: String,
}
#[derive(Hash, PartialEq)]
struct Statusline {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,

    model: StatuslineModel,
}
#[derive(Hash, PartialEq)]
struct LeftGutter {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightGutter {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct BottomOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct LeftOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopRightOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}

#[derive(Default, Hash, PartialEq)]
struct EditorTextArea {
    extent: PointU32,
}

struct EditorModel {
    scale: SizeScale, // Size of a line, in native pixels
    extent: PointU32, // In native pixels, whatever that is
    view_id: usize,
    main_font_metrics: Metrics,
}
struct EditorContainer {
    top: EditorTop,
    bottom: Statusline,
    left: LeftGutter,
    right: RightGutter, // Scrollbar, minimap etc.
    /*    top_overlay: TopOverlay,
    bottom_overlay: BottomOverlay,
    left_overlay: LeftOverlay,
    right_overlay: RightOverlay,
    topright_overlay: TopRightOverlay,*/
    center_text: EditorTextArea,
    model: EditorModel,
}

impl From<SizeScale> for f32 {
    fn from(value: SizeScale) -> Self {
        f32::from(value.line_height)
    }
}
impl From<SizeScale> for u32 {
    fn from(value: SizeScale) -> Self {
        f32::from(value.line_height) as u32
    }
}

impl SizeScale {
    fn round_up(&self) -> u32 {
        self.line_height.ceil() as u32
    }
    fn round_down(&self) -> u32 {
        self.line_height.floor() as u32
    }
}

impl EditorContainer {
    pub fn new(line_height: f32, font_info: Metrics) -> Self {
        let line_scale = SizeScale {
            line_height: OrderedFloat(line_height),
        };

        Self {
            model: EditorModel {
                scale: line_scale.clone(),
                extent: PointU32::default(),
                view_id: 0,
                main_font_metrics: font_info,
            },
            top: EditorTop {
                scale: line_scale.clone(),
                block: ShadowMetaContainerBlock::new(
                    RenderBlockId::normal(10).unwrap(),
                    PointF16::default(),
                    false,
                    None,
                ),
            },
            bottom: Statusline {
                scale: line_scale.clone(),
                block: ShadowMetaContainerBlock::new(
                    RenderBlockId::normal(11).unwrap(),
                    PointF16::default(),
                    false,
                    None,
                ),
            },
            left: LeftGutter {
                scale: line_scale.clone(),
                block: ShadowMetaContainerBlock::new(
                    RenderBlockId::normal(12).unwrap(),
                    PointF16::default(),
                    false,
                    None,
                ),
            },
            right: RightGutter {
                scale: line_scale.clone(),
                block: ShadowMetaContainerBlock::new(
                    RenderBlockId::normal(13).unwrap(),
                    PointF16::default(),
                    false,
                    None,
                ),
            },
            /*            top_overlay: TopOverlay {},
            bottom_overlay: BottomOverlay {},
            left_overlay: LeftOverlay {},
            right_overlay: RightOverlay {},-
            topright_overlay: TopRightOverlay {},*/
            center_text: EditorTextArea::default(),
        }
    }

    pub fn set_size(&mut self, extent: PointU32) {
        self.model.extent = extent;
        self.lay_out();
    }

    pub fn lay_out(&mut self) {
        let metrics = self.model.main_font_metrics;
        /* Updates layout sizes of the different elements */
        self.top.set_layout(
            self.model.scale.clone(),
            PointU32::new(self.model.extent.x(), 0),
        );
        self.bottom.set_layout(
            self.model.scale.clone(),
            PointU32::new(self.model.extent.x(), 0),
        );
        self.left.set_layout(
            self.model.scale.clone(),
            PointU32::new(
                (metrics.average_width
                    * (f32::from(self.model.scale.line_height)
                        / (metrics.ascent + metrics.descent))) as u32,
                self.model.extent.y(),
            ),
        );
        self.right.set_layout(
            self.model.scale.clone(),
            PointU32::new(0, self.model.extent.y()),
        );
        let horizontal_left = self
            .model
            .extent
            .y()
            .saturating_sub(self.top.extent().y())
            .saturating_sub(self.bottom.extent().y());
        let vertical_left = self
            .model
            .extent
            .x()
            .saturating_sub(self.right.extent().x())
            .saturating_sub(self.left.extent().x());
        self.center_text.set_layout(
            self.model.scale.clone(),
            PointU32::new(horizontal_left, vertical_left),
        );
    }
}

impl GfxComposibleBlock for EditorTop {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        /* Only the external width is used, for height we use line height * 1.5
        to have space for aline and some decoration. */
        self.block.set_extent(PointF16::new(
            extent.x() as f32,
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self) {}
}
impl GfxComposibleBlock for Statusline {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        /* Only the external width is used, for height we use line height * 1.5
        to have space for aline and some decoration. */
        self.block.set_extent(PointF16::new(
            extent.x() as f32,
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self) {
        /* Update meta shadow block based on any changes to local data / model */
    }
}

impl GfxComposibleBlock for LeftGutter {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.block.set_extent(PointF16::new(
            extent.x() as f32 * 6f32, // Occupy 6 average width letters
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self) {}
}
impl GfxComposibleBlock for RightGutter {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.block.set_extent(PointF16::new(
            extent.x() as f32 * 2.0f32, // Occupy 2 average width letters
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self) {}
}
/*
impl GfxComposibleBlock for TopOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for BottomOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for LeftOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for RightOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for TopRightOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}*/
impl GfxComposibleBlock for EditorTextArea {
    fn extent(&self) -> PointU32 {
        self.extent
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.extent = extent;
    }

    fn render(&mut self) {}
}
