//! Real layered-file saves and reloads retain the engine's saved history identity.
use photocraft_automation::Headless;
use serde_json::{Value, json};

fn command(h: &mut Headless, name: &str) -> Value {
    h.handle("engine.execute", json!({"command": name, "params": {}})).unwrap()
}

fn dirty(h: &mut Headless) -> bool {
    let i = h.session.active_index().unwrap();
    h.handle("session.list", json!({})).unwrap()["documents"][i]["dirty"].as_bool().unwrap()
}

#[test]
fn written_and_reopened_savepoints_follow_undo_and_redo() {
    let dir = std::env::temp_dir().join(format!(
        "pc-saved-history-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["pcraft", "psd", "psb"] {
        let path = dir.join(format!("saved.{ext}"));
        let mut h = Headless::trusted_local();
        h.handle("doc.new", json!({"width": 4, "height": 4, "background": "white"})).unwrap();
        command(&mut h, "image.adjustments.invert");
        h.handle("doc.save", json!({"path": path.to_str().unwrap()})).unwrap();
        assert!(!dirty(&mut h), "{ext}: successful real save is clean");
        assert!(path.metadata().unwrap().len() > 0);
        command(&mut h, "edit.undo");
        assert!(dirty(&mut h), "{ext}: white differs from saved black");
        command(&mut h, "edit.redo");
        assert!(!dirty(&mut h), "{ext}: redo returns to saved black");
        command(&mut h, "image.adjustments.invert");
        assert!(dirty(&mut h));
        let blocker = dir.join(format!("not-a-directory-{ext}"));
        std::fs::write(&blocker, b"block").unwrap();
        assert!(h.handle("doc.save", json!({"path": blocker.join(format!("fail.{ext}")).to_str().unwrap()})).is_err());
        assert!(dirty(&mut h), "{ext}: failed save must not move the savepoint");
        command(&mut h, "edit.undo");
        assert!(!dirty(&mut h), "{ext}: the earlier successful save remains the savepoint");
        h.handle("doc.open", json!({"path": path.to_str().unwrap()})).unwrap();
        assert!(!dirty(&mut h));
        assert_eq!(h.handle("engine.execute", json!({"command": "document.pixel", "params": {"x": 1, "y": 1}})).unwrap(), json!([0.0, 0.0, 0.0, 1.0]));
        command(&mut h, "image.adjustments.invert");
        assert!(dirty(&mut h));
        command(&mut h, "edit.undo");
        assert!(!dirty(&mut h), "{ext}: undo to the reopened document is clean");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
