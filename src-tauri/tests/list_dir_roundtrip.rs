use coding_tools_mcp_desktop_lib::tools::{call_tool, ToolContext};
use serde_json::{json, Value};

fn fixture() -> (tempfile::TempDir, ToolContext) {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(workspace.join("nested")).unwrap();
    std::fs::write(workspace.join("root.txt"), "root content\n").unwrap();
    std::fs::write(workspace.join("nested/中文 file.txt"), "nested content\n").unwrap();
    let ctx = ToolContext::for_test(workspace, root.path().join("state")).unwrap();
    (root, ctx)
}

fn assert_files_roundtrip(ctx: &ToolContext, listing: Value) {
    assert_eq!(listing["ok"], true, "{listing}");
    let entries = listing["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    for entry in entries {
        let path = entry["path"].as_str().unwrap();
        assert!(!std::path::Path::new(path).is_absolute(), "{path}");
        assert!(!path.starts_with('/'), "{path}");
        if entry["type"] == "file" {
            let read = call_tool(ctx, "read_file", &json!({"path":path}));
            assert_eq!(read["ok"], true, "listed path must be readable: {read}");
            assert!(read["content"].as_str().unwrap().ends_with("content\n"));
        }
    }
}

#[test]
fn default_and_dot_root_entries_are_workspace_relative_and_readable() {
    let (_root, ctx) = fixture();
    for args in [json!({}), json!({"path":"."})] {
        assert_files_roundtrip(&ctx, call_tool(&ctx, "list_dir", &args));
    }
}

#[test]
fn recursive_root_preserves_nested_unicode_paths() {
    let (_root, ctx) = fixture();
    let listing = call_tool(&ctx, "list_dir", &json!({"path":".","recursive":true,"max_depth":3}));
    assert!(listing["entries"].as_array().unwrap().iter()
        .any(|entry| entry["path"] == "nested/中文 file.txt"), "{listing}");
    assert_files_roundtrip(&ctx, listing);
}

#[test]
fn explicit_subdirectory_keeps_its_prefix() {
    let (_root, ctx) = fixture();
    let listing = call_tool(&ctx, "list_dir", &json!({"path":"nested"}));
    assert_eq!(listing["entries"][0]["path"], "nested/中文 file.txt");
    assert_files_roundtrip(&ctx, listing);
}

#[cfg(unix)]
#[test]
fn relative_listing_does_not_authorize_symlink_escape() {
    let (root, ctx) = fixture();
    let outside = root.path().join("outside.txt");
    std::fs::write(&outside, "not workspace data").unwrap();
    std::os::unix::fs::symlink(&outside, ctx.workspace.root().join("escape.txt")).unwrap();
    let listing = call_tool(&ctx, "list_dir", &json!({"path":"."}));
    if let Some(entry) = listing["entries"].as_array().unwrap().iter()
        .find(|entry| entry["name"] == "escape.txt") {
        assert_eq!(entry["path"], "escape.txt");
        let read = call_tool(&ctx, "read_file", &json!({"path":entry["path"]}));
        assert_eq!(read["ok"], false, "{read}");
    }
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "not workspace data");
}
