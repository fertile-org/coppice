use std::path::PathBuf;

/// `coppice-server desktop --data-dir <D> --resources <R>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopArgs {
    pub data_dir: PathBuf,
    pub resources: PathBuf,
}

/// `None` unless `args[1] == "desktop"`; both flags are required.
pub fn parse_desktop_args(args: &[String]) -> Result<Option<DesktopArgs>, String> {
    if args.get(1).map(String::as_str) != Some("desktop") {
        return Ok(None);
    }
    let mut data_dir = None;
    let mut resources = None;
    let mut rest = args[2..].iter();
    while let Some(flag) = rest.next() {
        let slot = match flag.as_str() {
            "--data-dir" => &mut data_dir,
            "--resources" => &mut resources,
            other => return Err(format!("unknown desktop argument {other:?}")),
        };
        let value = rest
            .next()
            .ok_or_else(|| format!("{flag} requires a path"))?;
        *slot = Some(PathBuf::from(value));
    }
    match (data_dir, resources) {
        (Some(data_dir), Some(resources)) => Ok(Some(DesktopArgs {
            data_dir,
            resources,
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
            }))
        );
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
