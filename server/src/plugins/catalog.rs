use std::io;
use std::path::Path;

/// Directory name of the first-party catalog inside the marketplace cache.
pub const CATALOG_DIR: &str = "coppice";

include!(concat!(env!("OUT_DIR"), "/marketplace_catalog.rs"));

/// Writes the embedded first-party catalog to `<dir>/coppice/`. The
/// `coppice` subtree is replaced wholesale so an upgrade refreshes and prunes
/// entries; other folders under `dir` are left alone.
pub fn materialize_catalog(dir: &Path) -> io::Result<()> {
    let catalog_root = dir.join(CATALOG_DIR);
    match std::fs::remove_dir_all(&catalog_root) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    for (rel, contents) in MARKETPLACE_FILES {
        let path = catalog_root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::capability::MARKETPLACE_JSON;
    use crate::plugins::manifest::PluginLayout;
    use crate::plugins::marketplace::expand_marketplace;

    #[test]
    fn every_embedded_entry_resolves() {
        let dir = tempfile::tempdir().unwrap();
        materialize_catalog(dir.path()).unwrap();
        let catalog = dir.path().join(CATALOG_DIR);
        assert!(catalog.join(MARKETPLACE_JSON).is_file());

        let rows = expand_marketplace(&catalog, CATALOG_DIR);
        assert_eq!(rows.len(), 1, "{rows:?}");
        for row in &rows {
            assert!(row.result.is_ok(), "{row:?}");
        }

        let github = rows[0].result.as_ref().unwrap();
        assert_eq!(rows[0].rel_path, "coppice/plugins/github");
        assert_eq!(github.name, "github");
        assert_eq!(github.layout, PluginLayout::Plugin);
        assert_eq!(github.marketplace.as_ref().unwrap().name, "coppice");
        assert_eq!(github.external, None);
        assert_eq!(github.mcp_servers.len(), 1);
        assert_eq!(github.mcp_servers[0].api_kind(), "http");
        assert!(github.skills.len() >= 4, "{:?}", github.skills);
        assert!(github.skills.iter().all(|s| s.error.is_none()));
    }

    #[test]
    fn materialize_replaces_stale_catalog_files() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join(CATALOG_DIR).join("plugins/gone");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("SKILL.md"), "stale").unwrap();
        materialize_catalog(dir.path()).unwrap();
        assert!(!stale.exists());
        assert!(dir.path().join("coppice/plugins/github/.mcp.json").is_file());
    }
}
