//! Host facts the Logs page and the later report link read.

/// Default until the retention setting exists. One of 3, 7, 14, 30.
pub const RETENTION_DAYS: u32 = 14;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn commit() -> &'static str {
    env!("COPPICE_BUILD_COMMIT")
}

pub fn os_name() -> &'static str {
    std::env::consts::OS
}

pub fn arch() -> &'static str {
    std::env::consts::ARCH
}

/// `PRETTY_NAME` from `/etc/os-release`, or `sw_vers -productVersion` on macOS.
/// Empty when that read fails.
pub fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        macos_product_version()
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_to_string("/etc/os-release")
            .map(|text| pretty_name(&text))
            .unwrap_or_default()
    }
}

#[cfg(target_os = "macos")]
fn macos_product_version() -> String {
    let output = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output();
    let Ok(output) = output else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(crate) fn pretty_name(contents: &str) -> String {
    for line in contents.lines() {
        let Some(value) = line.strip_prefix("PRETTY_NAME=") else {
            continue;
        };
        return unquote(value.trim());
    }
    String::new()
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_name_reads_the_quoted_value() {
        let text = "NAME=Debian\nPRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\n";
        assert_eq!(pretty_name(text), "Debian GNU/Linux 12 (bookworm)");
    }

    #[test]
    fn pretty_name_is_empty_when_the_key_is_missing() {
        assert_eq!(pretty_name("NAME=Debian\n"), "");
    }
}
