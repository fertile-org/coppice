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

/// Fails when the marketplace cache is the same as, or inside, any plugin dir.
/// A rescan would otherwise find every catalog entry as a plugin row.
pub fn check_cache_outside_plugin_dirs(cache: &Path, plugin_dirs: &[&Path]) -> io::Result<()> {
    let cache = std::fs::canonicalize(cache)?;
    for dir in plugin_dirs {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if cache.starts_with(&dir) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "marketplaces_dir {} must not be inside plugin dir {}",
                    cache.display(),
                    dir.display()
                ),
            ));
        }
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
    fn cache_inside_plugin_dir_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let plugins = root.path().join("plugins");
        let cache = plugins.join("marketplaces");
        std::fs::create_dir_all(&cache).unwrap();

        let err = check_cache_outside_plugin_dirs(&cache, &[plugins.as_path()])
            .expect_err("cache inside plugin dir must fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(check_cache_outside_plugin_dirs(&cache, &[cache.as_path()]).is_err());
    }

    #[test]
    fn cache_beside_plugin_dir_is_accepted() {
        let root = tempfile::tempdir().unwrap();
        let plugins = root.path().join("plugins");
        let cache = root.path().join("marketplaces");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::create_dir_all(&cache).unwrap();

        check_cache_outside_plugin_dirs(&cache, &[plugins.as_path()]).expect("sibling is fine");
    }

    #[test]
    fn cache_check_ignores_missing_plugin_dir_path() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("marketplaces");
        std::fs::create_dir_all(&cache).unwrap();
        let missing = root.path().join("gone");

        check_cache_outside_plugin_dirs(&cache, &[missing.as_path()]).expect("missing dir");
    }

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
        assert!(dir
            .path()
            .join("coppice/plugins/github/.mcp.json")
            .is_file());
    }
}
