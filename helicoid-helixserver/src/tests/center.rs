use std::{iter::once, sync::Arc};

use helicoid_protocol::{
    gfx::{PointF32, RenderBlockId, RenderBlockLocation, RenderBlockPath},
    shadowblocks::{ShadowMetaBlock, ShadowMetaContainerBlock},
    transferbuffer::TransferBuffer,
};
use helix_core::{
    movement::{move_vertically, Direction},
    SmartString, Transaction,
};
use helix_view::{Editor as VEditor, ViewId};

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
fn move_selection(
    dy: Direction,
    count: usize,
    editor: &mut VEditor,
    view_id: ViewId,
    viewport_width: u16,
) {
    let view = editor.tree.get(view_id);
    let doc_id = view.doc.clone();
    let doc = editor.documents.get(&doc_id).unwrap();
    let text = doc.text().slice(..);
    let text_fmt = doc.text_format(viewport_width, None);
    let mut selection = doc.selection(view.id).clone();
    let old_selection = selection.clone();
    let mut annotations = view.text_annotations(doc, None);
    selection = selection.transform(|range| {
        move_vertically(
            text,
            range,
            dy,
            count,
            helix_core::movement::Movement::Move,
            &text_fmt,
            &mut annotations,
        )
    });
    let doc_mut = editor.documents.get_mut(&doc_id).unwrap();
    log::debug!(
        "Moving section: x: {:?} y: {:?} {:?} -> {:?}",
        0,
        dy,
        old_selection,
        selection
    );
    doc_mut.set_selection(view_id, selection);
}

async fn update_blocked(
    mut block: ShadowMetaBlock<ContentVisitor>,
    mut visitor: ContentVisitor,
) -> (ShadowMetaBlock<ContentVisitor>, ContentVisitor) {
    tokio::task::spawn_blocking(move || {
        block.container_mut().unwrap().update(&mut visitor);
        (block, visitor)
    })
    .await
    .unwrap()
}

#[test_log::test(tokio::test)]
async fn center_scoll() {
    let center_model = CenterModel::default();
    let mut block = ShadowMetaContainerBlock::new(
        CENTER_MODEL_CONTAINER_ID,
        PointF32::new(10f32, 20f32),
        false,
        None,
        center_model,
    );
    block.set_extent(PointF32::new(100f32, 100f32));

    let mut content_visitor = prepare_content_visitor();
    let view_id = load_dummy_view(
        &mut content_visitor,
        Some(
            &"Some dummy text
        \n\n\n\n\nLine\n\n\nText\nLorem\nIpsum\nEst\nDisputandum\nconst\nmut\nlet",
        ),
    )
    .await
    .unwrap();

    let (block, content_visitor) = tokio::task::spawn_blocking(move || {
        block.initialize(&mut content_visitor);
        block.update(&mut content_visitor);
        (block, content_visitor)
    })
    .await
    .unwrap();

    let mut loc = RenderBlockLocation {
        id: CENTER_MODEL_CONTAINER_ID,
        location: PointF32::new(25f32, 32f32),
        layer: 0,
    };
    let mut transfer_buffer = TransferBuffer::new();
    let mut wrapped_block = ShadowMetaBlock::WrappedContainer(Box::new(block));
    wrapped_block.client_transfer_messages(
        &CENTER_BLOCK_PARENT_PATH,
        &mut loc,
        &mut transfer_buffer,
    );

    log::debug!(
        "Messages to transfer to client pre move: {:?}",
        transfer_buffer
    );
    log::debug!("---------------------------------------------------------");

    // TODO: Examine out_messages

    {
        let mut editor = content_visitor.editor().lock().await;
        let heditor = editor.editor_mut();
        move_selection(
            Direction::Forward,
            10,
            heditor,
            view_id,
            wrapped_block.extent().x() as u16,
        );
    }

    let (mut wrapped_block, _content_visitor) =
        update_blocked(wrapped_block, content_visitor).await;

    transfer_buffer.clear();
    wrapped_block.client_transfer_messages(
        &CENTER_BLOCK_PARENT_PATH,
        &mut loc,
        &mut transfer_buffer,
    );
    log::debug!(
        "Messages to transfer to client post move: Move: {:?} Update: {:?}",
        transfer_buffer
            .moves()
            .iter()
            .map(|(pt, mv)| {
                format!(
                    "Path: {:x?} Moves: {:x?}",
                    pt.path(),
                    mv.iter().map(|loc| loc.id.0).collect::<Vec<_>>()
                )
            })
            .collect::<Vec<_>>(),
        transfer_buffer
            .additions()
            .iter()
            .map(|(pt, nv)| {
                format!(
                    "Path: {:x?} Additions: {:x?}",
                    pt.path(),
                    nv.iter().map(|nv| nv.id.0).collect::<Vec<_>>()
                )
            })
            .collect::<Vec<_>>()
    );
    let result = 2 + 2;
    assert_eq!(result, 4);
}
