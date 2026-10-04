use std::path::{Path, PathBuf};

/// Per-user writable data dir (`--data-dir`).
#[derive(Debug, Clone)]
pub struct DataLayout {
    pub root: PathBuf,
    pub config_file: PathBuf,
    pub secrets_dir: PathBuf,
    pub pg_data: PathBuf,
    pub pg_log: PathBuf,
    pub artifacts: PathBuf,
    pub worktrees: PathBuf,
    pub plugins: PathBuf,
    pub builtin_plugins: PathBuf,
    pub logs: PathBuf,
}

impl DataLayout {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            config_file: root.join("config.toml"),
            secrets_dir: root.join("secrets"),
            pg_data: root.join("pg").join("data"),
            pg_log: root.join("logs").join("postgres.log"),
            artifacts: root.join("artifacts"),
            worktrees: root.join("worktrees"),
            plugins: root.join("plugins"),
            builtin_plugins: root.join("builtin-plugins"),
            logs: root.join("logs"),
        }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in [
            &self.root,
            &self.secrets_dir,
            &self.pg_data,
            &self.artifacts,
            &self.worktrees,
            &self.plugins,
            &self.builtin_plugins,
            &self.logs,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Read-only bundle shipped with the app (`--resources`).
#[derive(Debug, Clone)]
pub struct ResourceLayout {
    pub root: PathBuf,
    pub pg_bin: PathBuf,
    pub pg_lib: PathBuf,
    pub web: PathBuf,
    pub agent_templates: PathBuf,
    pub mock_fixtures: PathBuf,
}

impl ResourceLayout {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            pg_bin: root.join("postgres").join("bin"),
            pg_lib: root.join("postgres").join("lib"),
            web: root.join("web"),
            agent_templates: root.join("agent-templates"),
            mock_fixtures: root.join("fixtures").join("agent-responses"),
        }
    }

    /// Errors with the first required path that is missing.
    pub fn validate(&self) -> Result<(), String> {
        let required = [
            self.pg_bin.join("initdb"),
            self.pg_bin.join("pg_ctl"),
            self.pg_bin.join("postgres"),
            self.web.join("index.html"),
            self.agent_templates.clone(),
        ];
        match required.iter().find(|path| !path.exists()) {
            Some(missing) => Err(format!(
                "desktop resources incomplete: missing {}",
                missing.display()
            )),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_layout_paths_are_under_root() {
        let layout = DataLayout::new(Path::new("/d"));
        assert_eq!(layout.config_file, Path::new("/d/config.toml"));
        assert_eq!(layout.secrets_dir, Path::new("/d/secrets"));
        assert_eq!(layout.pg_data, Path::new("/d/pg/data"));
        assert_eq!(layout.pg_log, Path::new("/d/logs/postgres.log"));
        assert_eq!(layout.builtin_plugins, Path::new("/d/builtin-plugins"));
    }

    #[test]
    fn ensure_dirs_creates_every_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = DataLayout::new(&dir.path().join("Coppice"));
        layout.ensure_dirs().expect("ensure dirs");
        for path in [
            &layout.secrets_dir,
            &layout.pg_data,
            &layout.artifacts,
            &layout.worktrees,
            &layout.plugins,
            &layout.builtin_plugins,
            &layout.logs,
        ] {
            assert!(path.is_dir(), "{} missing", path.display());
        }
    }

    #[test]
    fn resource_validate_names_initdb_first_on_empty_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = ResourceLayout::new(dir.path())
            .validate()
            .expect_err("empty resources must fail");
        assert!(err.contains("initdb"), "{err}");
    }

    #[test]
    fn resource_validate_accepts_complete_bundle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let resources = ResourceLayout::new(dir.path());
        std::fs::create_dir_all(&resources.pg_bin).unwrap();
        std::fs::create_dir_all(&resources.web).unwrap();
        std::fs::create_dir_all(&resources.agent_templates).unwrap();
        for bin in ["initdb", "pg_ctl", "postgres"] {
            std::fs::write(resources.pg_bin.join(bin), "").unwrap();
        }
        std::fs::write(resources.web.join("index.html"), "").unwrap();
        resources.validate().expect("complete bundle");
    }

    #[test]
    fn resource_validate_names_web_index_when_only_postgres_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let resources = ResourceLayout::new(dir.path());
        std::fs::create_dir_all(&resources.pg_bin).unwrap();
        for bin in ["initdb", "pg_ctl", "postgres"] {
            std::fs::write(resources.pg_bin.join(bin), "").unwrap();
        }
        let err = resources.validate().expect_err("web missing");
        assert!(err.contains("index.html"), "{err}");
    }
}
