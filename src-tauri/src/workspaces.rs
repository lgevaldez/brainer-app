use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub project_like: bool,
}

fn is_project_like_dir(path: &Path) -> bool {
    if path.join(".git").exists() {
        return true;
    }

    [
        "README.md",
        "package.json",
        "pyproject.toml",
        "Cargo.toml",
        "go.mod",
        "composer.json",
    ]
    .iter()
    .any(|name| path.join(name).exists())
}

fn classify_dir(path: &Path) -> (String, bool) {
    if path.join(".git").exists() {
        return ("git_repo".to_string(), true);
    }
    if is_project_like_dir(path) {
        return ("project_like".to_string(), true);
    }
    ("folder".to_string(), false)
}

pub fn discover_workspace_children_entries(root: PathBuf) -> Result<Vec<FolderEntry>, String> {
    let mut entries = vec![];
    let dir = fs::read_dir(root).map_err(|err| format!("Failed to read workspace root: {}", err))?;
    for entry in dir {
        let entry = entry.map_err(|err| format!("Read dir entry error: {}", err))?;
        let child_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') || !child_path.is_dir() {
            continue;
        }
        let (kind, project_like) = classify_dir(&child_path);
        entries.push(FolderEntry {
            name: file_name,
            path: child_path.to_string_lossy().to_string(),
            kind,
            project_like,
        });
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("brainer-app-{}-{}", name, millis))
    }

    #[test]
    fn discovers_and_classifies_project_like_folders() {
        let root = temp_dir("workspace");
        let repo = root.join("repo-a");
        let plain = root.join("notes");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(&plain).unwrap();
        fs::write(repo.join("README.md"), "# repo").unwrap();

        let entries = discover_workspace_children_entries(root.clone()).unwrap();

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|item| item.name == "repo-a" && item.project_like));
        assert!(entries.iter().any(|item| item.name == "notes" && !item.project_like));

        fs::remove_dir_all(root).unwrap();
    }
}
