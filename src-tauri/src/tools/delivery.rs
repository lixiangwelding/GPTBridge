//! Local delivery is metadata, not another write path or a download service.
use std::path::{Component, Path};
use serde_json::{json,Value};

pub fn policy(root:&Path)->Value {
    json!({"mode":"local_files","workspace_absolute_path":root,
        "default_directory":root.join("docs/deliverables"),"directory_created":false,
        "prefer_existing_project_routes":true,"return_absolute_paths":true,
        "download_required":false,"instruction":"Write requested artifacts through managed tools in this local workspace, verify them and return absolute paths. Prefer existing project output routes; use docs/deliverables only when none exists. Never invent a local path for a cloud-only file."})
}

pub fn attach_patch(root:&Path,args:&Value,output:&mut Value) {
    if output["ok"]!=true || output["dry_run"]==true || args["dry_run"]==true {return;}
    let mut files=Vec::new();
    for item in output["affected_files"].as_array().into_iter().flatten() {
        if !matches!(item["operation"].as_str(),Some("add"|"update")){continue;}
        let Some(relative)=item["path"].as_str() else {continue;};
        let path=Path::new(relative);
        if path.is_absolute() || !path.components().all(|c|matches!(c,Component::Normal(_))) {continue;}
        let absolute=root.join(path);
        let present=absolute.is_file() && absolute.canonicalize().is_ok_and(|p|p.starts_with(root));
        if !present {continue;}
        files.push(json!({"path":relative,"absolute_path":absolute,
            "receipt_sha256":output["after_hashes"][relative],"exists_at_response":true,
            "current_content_reverified":false}));
    }
    output["local_delivery"]=json!({"mode":"local_files","files":files,
        "download_required":false,"note":"Paths were verified present within the workspace; receipt hashes describe the write, not a later concurrent edit."});
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture()->(tempfile::TempDir,std::path::PathBuf,Value) {
        let dir=tempfile::tempdir().unwrap();let root=dir.path().canonicalize().unwrap();
        std::fs::write(root.join("report.md"),"report").unwrap();
        let output=json!({"ok":true,"dry_run":false,"affected_files":[{"operation":"add","path":"report.md"}],"after_hashes":{"report.md":"receipt-hash"}});
        (dir,root,output)
    }
    #[test]
    fn successful_patch_returns_existing_absolute_path_and_preserves_receipt() {
        let (_dir,root,mut value)=fixture();attach_patch(&root,&json!({}),&mut value);
        assert_eq!(value["local_delivery"]["files"][0]["absolute_path"],root.join("report.md").to_string_lossy().as_ref());
        assert_eq!(value["after_hashes"]["report.md"],"receipt-hash");
        assert_eq!(value["local_delivery"]["download_required"],false);
    }
    #[test]
    fn preview_failure_and_deleted_files_are_not_delivered() {
        let (_dir,root,original)=fixture();
        for (args,mut value) in [(json!({"dry_run":true}),original.clone()),(json!({}),json!({"ok":false})),(json!({}),json!({"ok":true,"dry_run":true}))] {
            attach_patch(&root,&args,&mut value);assert!(value.get("local_delivery").is_none());
        }
        let mut value=original;value["affected_files"][0]["operation"]=json!("delete");
        attach_patch(&root,&json!({}),&mut value);assert!(value["local_delivery"]["files"].as_array().unwrap().is_empty());
    }
    #[test]
    fn missing_and_outside_paths_are_not_claimed_as_local() {
        let (_dir,root,mut value)=fixture();
        for path in ["../report.md","/etc/passwd","missing.md"] {
            value["affected_files"][0]["path"]=json!(path);attach_patch(&root,&json!({}),&mut value);
            assert!(value["local_delivery"]["files"].as_array().unwrap().is_empty());
        }
    }
    #[test]
    fn policy_does_not_create_directories() {
        let (_dir,root,_)=fixture();let p=policy(&root);
        assert_eq!(p["directory_created"],false);assert!(!root.join("docs/deliverables").exists());
    }
}
