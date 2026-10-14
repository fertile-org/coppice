use std::path::PathBuf;

/// `coppice-server desktop --data-dir <D> --resources <R> [--logs-dir <L>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopArgs {
    pub data_dir: PathBuf,
    pub resources: PathBuf,
    pub logs_dir: Option<PathBuf>,
}

/// `None` unless `args[1] == "desktop"`; both flags are required.
pub fn parse_desktop_args(args: &[String]) -> Result<Option<DesktopArgs>, String> {
    if args.get(1).map(String::as_str) != Some("desktop") {
        return Ok(None);
    }
    let mut data_dir = None;
    let mut resources = None;
    let mut logs_dir = None;
    let mut rest = args[2..].iter();
    while let Some(flag) = rest.next() {
        let value = rest
            .next()
            .ok_or_else(|| format!("{flag} requires a path"))?;
        match flag.as_str() {
            "--data-dir" => data_dir = Some(PathBuf::from(value)),
            "--resources" => resources = Some(PathBuf::from(value)),
            "--logs-dir" => logs_dir = Some(PathBuf::from(value)),
            other => return Err(format!("unknown desktop argument {other:?}")),
        }
    }
    match (data_dir, resources) {
        (Some(data_dir), Some(resources)) => Ok(Some(DesktopArgs {
            data_dir,
            resources,
            logs_dir,
        })),
        (None, _) => Err("desktop requires --data-dir <path>".into()),
        (_, None) => Err("desktop requires --resources <path>".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_subcommand_is_not_desktop() {
        assert_eq!(parse_desktop_args(&strings(&["coppice-server"])), Ok(None));
        assert_eq!(
            parse_desktop_args(&strings(&["coppice-server", "serve"])),
            Ok(None)
        );
    }

    #[test]
    fn desktop_with_both_paths() {
        let args = strings(&["x", "desktop", "--data-dir", "/d", "--resources", "/r"]);
        assert_eq!(
            parse_desktop_args(&args),
            Ok(Some(DesktopArgs {
                data_dir: PathBuf::from("/d"),
                resources: PathBuf::from("/r"),
                logs_dir: None,
            }))
        );
    }

    #[test]
    fn logs_dir_is_optional_and_kept() {
        let args = strings(&[
            "x",
            "desktop",
            "--logs-dir",
            "/var/log/Coppice",
            "--resources",
            "/r",
            "--data-dir",
            "/d",
        ]);
        let parsed = parse_desktop_args(&args).expect("parse").expect("desktop");
        assert_eq!(parsed.logs_dir, Some(PathBuf::from("/var/log/Coppice")));
        assert_eq!(parsed.data_dir, PathBuf::from("/d"));
    }

    #[test]
    fn missing_resources_is_an_error_naming_it() {
        let err = parse_desktop_args(&strings(&["x", "desktop", "--data-dir", "/d"]))
            .expect_err("missing --resources");
        assert!(err.contains("--resources"), "{err}");
    }

    #[test]
    fn missing_value_and_unknown_flag_are_errors() {
        let err = parse_desktop_args(&strings(&[
            "x",
            "desktop",
            "--resources",
            "/r",
            "--data-dir",
        ]))
        .expect_err("missing value");
        assert!(err.contains("--data-dir"), "{err}");
        let err = parse_desktop_args(&strings(&["x", "desktop", "--port", "1"]))
            .expect_err("unknown flag");
        assert!(err.contains("--port"), "{err}");
    }
}
