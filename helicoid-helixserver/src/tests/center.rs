use std::{env, iter::once, sync::Arc};

use helicoid_protocol::{
    caching_shaper::CachingShaper,
    dataflow::{ShadowMetaBlock, ShadowMetaContainerBlock},
    gfx::{PointF16, RemoteBoxUpdate, RenderBlockId, RenderBlockLocation, RenderBlockPath},
};
use helix_core::{SmartString, Transaction};
use helix_view::{Document, ViewId};
use ordered_float::OrderedFloat;
use tokio::sync::Mutex as TMutex;

use crate::{
    center::CenterModel, editor::Editor, editor_view::ContentVisitor, server::HelicoidServer,
};

const CENTER_MODEL_CONTAINER_ID: RenderBlockId = RenderBlockId(0xFE);
lazy_static! {
    static ref CENTER_BLOCK_PARENT_PATH: RenderBlockPath =
        RenderBlockPath::new(smallvec::smallvec![RenderBlockId(10), RenderBlockId(12)]);
}

fn prepare_content_visitor() -> ContentVisitor {
    let editor = Editor::new();
    let locked_editor = Arc::new(TMutex::new(editor));

    HelicoidServer::make_content_visitor(1.5, locked_editor)
}

/* Loads a document, optionaly with the provided initial text, and sets up a view */
async fn load_dummy_view(
    visitor: &mut ContentVisitor,
    intial_text: Option<&str>,
) -> Option<ViewId> {
    let mut editor = visitor.editor().lock().await;
    let heditor = editor.editor_mut();
    let doc_id = heditor.new_file(helix_view::editor::Action::VerticalSplit);
    let doc = heditor.documents.get_mut(&doc_id).unwrap();
    let view_id = heditor.tree.focus;
    assert_eq!(heditor.tree.get(view_id).doc, doc_id);

    if let Some(initial_text) = intial_text {
        let insert_initial_text = Transaction::change(
            doc.text(),
            once((0, 0, Some(SmartString::from(initial_text)))),
        );

        assert!(doc.apply(&insert_initial_text, view_id))
    }
    drop(editor);
    visitor.set_active_view_id(Some(view_id));
    Some(view_id)
}

#[test_env_log::test(tokio::test)]
async fn center_scoll() {
    env::set_var("RUST_LOG", "trace");
    let mut center_model = CenterModel::default();
    center_model.scaled_font_size = OrderedFloat::<f32>(16f32);
    let mut block = ShadowMetaContainerBlock::new(
        CENTER_MODEL_CONTAINER_ID,
        PointF16::new(10f32, 20f32),
        false,
        None,
        center_model,
    );
    block.set_extent(PointF16::new(100f32, 100f32));

    let mut content_visitor = prepare_content_visitor();
    let _view_id = load_dummy_view(&mut content_visitor, Some(&"Some dummy text"))
        .await
        .unwrap();

    let (block, _content_visitor) = tokio::task::spawn_blocking(move || {
        block.initialize(&mut content_visitor);
        block.update(&mut content_visitor);
        (block, content_visitor)
    })
    .await
    .unwrap();

    let mut loc = RenderBlockLocation {
        id: CENTER_MODEL_CONTAINER_ID,
        location: PointF16::new(25f32, 32f32),
        layer: 0,
    };
    let mut out_messages = Vec::<RemoteBoxUpdate>::with_capacity(100);
    let mut wrapped_block = ShadowMetaBlock::WrappedContainer(Box::new(block));
    wrapped_block.client_transfer_messages(&CENTER_BLOCK_PARENT_PATH, &mut loc, &mut out_messages);

    // TODO: Examine out_messages
    let result = 2 + 2;
    assert_eq!(result, 4);
}
