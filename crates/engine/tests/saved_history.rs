//! Dirty state follows the saved history state, while render revisions keep increasing.
use photocraft_engine::Session;
use serde_json::{Value, json};

fn session(depth: u32) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 4, "height": 4, "depth": depth, "background": "white"})).unwrap();
    s
}

fn pixel(s: &mut Session) -> Value {
    s.execute("document.pixel", json!({"x": 1, "y": 1})).unwrap()
}

#[test]
fn undo_to_open_state_is_clean_and_revision_keeps_increasing() {
    for depth in [8, 16, 32] {
        let mut s = session(depth);
        let original = pixel(&mut s);
        let opened_revision = s.active().unwrap().revision;
        s.execute("image.adjustments.invert", json!({})).unwrap();
        let edited_revision = s.active().unwrap().revision;
        assert!(s.active().unwrap().is_dirty());
        assert_ne!(pixel(&mut s), original);
        assert!(s.undo());
        assert_eq!(pixel(&mut s), original, "Undo restored the exact saved pixels");
        assert!(s.active().unwrap().revision > edited_revision);
        assert!(edited_revision > opened_revision);
        assert!(!s.active().unwrap().is_dirty(), "Undo to the opened state must clear dirty");
        assert!(s.redo());
        assert!(s.active().unwrap().is_dirty());
    }
}

#[test]
fn redo_to_saved_state_is_clean() {
    let mut s = session(8);
    s.execute("image.adjustments.invert", json!({})).unwrap();
    s.active_mut().unwrap().saved_to("saved.pcraft".into());
    let saved = pixel(&mut s);
    assert!(s.undo());
    assert!(s.active().unwrap().is_dirty());
    assert_ne!(pixel(&mut s), saved);
    assert!(s.redo());
    assert_eq!(pixel(&mut s), saved);
    assert!(!s.active().unwrap().is_dirty(), "Redo to the saved state must clear dirty");
}

#[test]
fn save_inside_a_coalesced_gesture_preserves_the_savepoint() {
    let mut s = session(8);
    s.execute("layer.setProps", json!({"opacity": 0.8, "coalesce": "opacity"})).unwrap();
    s.active_mut().unwrap().saved_to("saved.pcraft".into());
    let saved = s.active().unwrap().doc.layers[0].opacity;
    s.execute("layer.setProps", json!({"opacity": 0.6, "coalesce": "opacity"})).unwrap();
    assert!(s.active().unwrap().is_dirty());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.layers[0].opacity, saved, "saving must end the prior coalesced step");
    assert!(!s.active().unwrap().is_dirty());
}

#[test]
fn repeated_undo_redo_crosses_the_savepoint_in_both_directions() {
    let mut s = session(16);
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.active_mut().unwrap().saved_to("saved.psd".into());
    s.execute("edit.fill", json!({"color": "#0000ff"})).unwrap();
    let mut revision = s.active().unwrap().revision;
    for _ in 0..3 {
        for (redo, dirty) in [(false, false), (false, true), (true, false), (true, true)] {
            assert!(if redo { s.redo() } else { s.undo() });
            let st = s.active().unwrap();
            assert_eq!(st.is_dirty(), dirty);
            assert!(st.revision > revision);
            revision = st.revision;
        }
    }
}

#[test]
fn a_branch_at_the_same_history_index_never_reuses_the_saved_state() {
    let mut s = session(8);
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.active_mut().unwrap().saved_to("saved.pcraft".into());
    assert!(s.undo());
    s.execute("edit.fill", json!({"color": "#0000ff"})).unwrap();
    assert!(s.active().unwrap().is_dirty());
    assert!(!s.redo());
    assert!(s.undo());
    assert!(s.active().unwrap().is_dirty());
    assert!(s.redo());
    assert!(s.active().unwrap().is_dirty());
}

#[test]
fn coalesced_edits_still_share_one_step_without_an_intervening_save() {
    let mut s = session(8);
    for opacity in [0.8, 0.6, 0.4] {
        s.execute("layer.setProps", json!({"opacity": opacity, "coalesce": "opacity"})).unwrap();
    }
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.layers[0].opacity, 1.0);
    assert!(!s.active().unwrap().is_dirty());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc.layers[0].opacity, 0.4);
    assert!(s.active().unwrap().is_dirty());
}

#[test]
fn count_and_byte_trimming_cannot_turn_a_different_state_clean() {
    for byte_budget in [false, true] {
        let mut s = session(8);
        s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        s.active_mut().unwrap().saved_to("saved.pcraft".into());
        if byte_budget {
            s.active_mut().unwrap().history.max_bytes = 1;
        } else {
            s.active_mut().unwrap().history.max_states = 1;
        }
        for color in ["#00ff00", "#0000ff", "#ffffff"] {
            s.execute("edit.fill", json!({"color": color})).unwrap();
        }
        assert_eq!(s.active().unwrap().history.past_len(), 1);
        assert!(s.undo());
        assert!(s.active().unwrap().is_dirty());
        assert!(!s.undo());
        assert!(s.redo());
        assert!(s.active().unwrap().is_dirty());
    }
}

#[test]
fn purge_keeps_current_savepoint_but_does_not_reuse_a_discarded_one() {
    let mut s = session(8);
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.active_mut().unwrap().saved_to("saved.pcraft".into());
    s.active_mut().unwrap().history.clear();
    assert!(!s.active().unwrap().is_dirty());
    s.execute("edit.fill", json!({"color": "#0000ff"})).unwrap();
    assert!(s.undo());
    assert!(!s.active().unwrap().is_dirty());
    assert!(s.redo());
    s.active_mut().unwrap().history.clear();
    s.execute("edit.fill", json!({"color": "#00ff00"})).unwrap();
    assert!(s.undo());
    assert!(s.active().unwrap().is_dirty());
}

#[test]
fn save_as_and_view_revisions_do_not_change_content_identity() {
    let mut s = session(8);
    s.execute("layer.new.layer", json!({})).unwrap();
    s.active_mut().unwrap().saved_to("renamed.psb".into());
    let layer = s.active().unwrap().doc.layers[0].id;
    s.select_layer(layer).unwrap();
    assert!(!s.active().unwrap().is_dirty());
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    assert!(s.undo());
    assert!(!s.active().unwrap().is_dirty());
    assert_eq!(s.active().unwrap().doc.name, "renamed.psb");
    assert_eq!(s.active().unwrap().path.as_deref(), Some("renamed.psb"));
    assert!(s.undo());
    assert!(s.active().unwrap().is_dirty());
    assert!(s.redo());
    assert!(!s.active().unwrap().is_dirty());
}

#[test]
fn a_failed_edit_does_not_move_or_erase_the_savepoint() {
    let mut s = session(8);
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.active_mut().unwrap().saved_to("saved.pcraft".into());
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let result: photocraft_engine::Result<()> = s.edit("Fail", |doc, _| {
        doc.name = "not committed".into();
        Err(photocraft_engine::EngineError::Other("injected failure".into()))
    });
    assert!(result.is_err());
    assert!(std::sync::Arc::ptr_eq(&before, &s.active().unwrap().doc));
    assert_eq!(s.active().unwrap().revision, revision);
    assert!(!s.active().unwrap().is_dirty());
    assert!(s.undo());
    assert!(s.active().unwrap().is_dirty());
    assert!(s.redo());
    assert!(!s.active().unwrap().is_dirty());
}

#[test]
fn untracked_mutation_after_save_does_not_revive_an_older_savepoint() {
    for direct_assignment in [false, true] {
        for command in ["timeline.create", "image.variables.dataSets"] {
            let mut s = session(8);
            s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
            let st = s.active_mut().unwrap();
            if direct_assignment {
                st.saved_revision = st.revision;
            } else {
                st.saved_to("saved.pcraft".into());
            }
            let params =
                if command == "timeline.create" { json!({"duration": 3, "fps": 12.0}) } else { json!({"dataSets": [{"name": "Set A", "values": []}]}) };
            s.execute(command, params).unwrap();
            assert!(s.active().unwrap().is_dirty());
            assert!(s.undo());
            assert!(s.active().unwrap().is_dirty(), "{command}: the opened white state differs from the saved red state");
            assert!(s.redo());
            assert!(s.active().unwrap().is_dirty(), "{command}: the red state includes unsaved metadata");
        }
    }
}

#[test]
fn recovered_document_never_returns_to_a_fictional_savepoint() {
    let mut s = session(8);
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.active_mut().unwrap().saved_revision = 0;
    assert!(s.undo());
    assert!(s.active().unwrap().is_dirty());
    assert!(s.redo());
    assert!(s.active().unwrap().is_dirty());
    s.active_mut().unwrap().mark_saved();
    assert!(s.undo());
    assert!(s.active().unwrap().is_dirty());
    assert!(s.redo());
    assert!(!s.active().unwrap().is_dirty());
}

#[test]
fn savepoints_belong_to_their_own_documents() {
    let mut s = session(8);
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.active_mut().unwrap().saved_to("first.pcraft".into());
    s.execute("file.new", json!({"width": 4, "height": 4})).unwrap();
    s.execute("edit.fill", json!({"color": "#0000ff"})).unwrap();
    assert!(s.documents()[1].is_dirty());
    assert!(s.set_active(0));
    assert!(s.undo());
    assert!(s.documents()[0].is_dirty());
    assert!(s.documents()[1].is_dirty());
    assert!(s.redo());
    assert!(!s.documents()[0].is_dirty());
    assert!(s.documents()[1].is_dirty());
    assert!(s.set_active(1));
    assert!(s.undo());
    assert!(!s.documents()[1].is_dirty());
    assert!(!s.documents()[0].is_dirty());
}
