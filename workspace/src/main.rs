use std::path::PathBuf;
use uuid::Uuid;
use workspace::{
    protocol::{SyncAction, SyncRecord},
    queue::SyncQueue,
    watch,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "workspace-data".to_string()),
    );

    std::fs::create_dir_all(&root)?;

    let workspace_id = Uuid::new_v4();
    let queue = SyncQueue::new(&root)?;

    println!("MeshOS Workspace Sync");
    println!("Workspace: {}", workspace_id);
    println!("Root: {}", root.display());
    println!("Pending events: {}", queue.load()?.len());

    let (_watcher, rx) = watch::start(&root)?;

    println!("[MeshOS] Workspace watcher started");
    println!("[MeshOS] Watching for changes...");

    for result in rx {
        match result {
            Ok(event) => {
                for change in watch::to_change(&root, workspace_id, event) {
                    let action = match change.kind.as_str() {
                        "created" | "modified" => SyncAction::Upsert,
                        "removed" => SyncAction::Delete,
                        _ => continue,
                    };

                    let record = SyncRecord {
                        action,
                        path: change.relative_path.clone(),
                        size: change.size,
                        sha256: change.sha256.clone(),
                        event_id: change.event_id.to_string(),
                    };

                    queue.push(record)?;

                    println!("[MeshOS] Change queued: {}", change.relative_path);
                }
            }

            Err(error) => {
                eprintln!("[MeshOS] Watcher error: {error}");
            }
        }
    }

    Ok(())
}
