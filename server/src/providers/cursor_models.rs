#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

/// Parse `agent models` / `agent --list-models` human text:
/// `id - Display Name` per line.
pub fn parse_cursor_models_stdout(stdout: &str) -> Vec<ModelOption> {
    let cleaned = strip_ansi(stdout);
    let mut models = Vec::new();
    for line in cleaned.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Available") || line.starts_with("Error") {
            continue;
        }
        let Some((id, name)) = split_id_name(line) else {
            continue;
        };
        let id = id.trim();
        let name = name.trim();
        if id.is_empty() || name.is_empty() || id.contains(' ') {
            continue;
        }
        models.push(ModelOption {
            id: id.to_string(),
            name: name.to_string(),
        });
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

fn split_id_name(line: &str) -> Option<(&str, &str)> {
    for sep in [" - ", " – ", " — "] {
        if let Some(parts) = line.split_once(sep) {
            return Some(parts);
        }
    }
    None
}

/// Strip CSI / OSC ANSI sequences so colorized CLI output still parses.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                for ch in chars.by_ref() {
                    if ch == '\u{7}' {
                        break;
                    }
                    if ch == '\u{1b}' {
                        // ST is ESC \
                        if matches!(chars.peek(), Some('\\')) {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

pub async fn list_cursor_models(command: &str) -> anyhow::Result<Vec<ModelOption>> {
    let output = tokio::process::Command::new(command)
        .arg("models")
        .output()
        .await
        .map_err(|err| anyhow::anyhow!("failed to spawn `{command} models`: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        anyhow::bail!(
            "`{command} models` exited {}: stdout={} stderr={}",
            output.status,
            truncate_for_err(&stdout),
            truncate_for_err(&stderr)
        );
    }

    // Some CLI builds print the table on stderr when stdout is not a TTY.
    let combined = if stdout.trim().is_empty() {
        stderr.as_ref()
    } else {
        stdout.as_ref()
    };
    let models = parse_cursor_models_stdout(combined);
    if models.is_empty() {
        anyhow::bail!(
            "no cursor models parsed from `{command} models` (stdout={} stderr={})",
            truncate_for_err(&stdout),
            truncate_for_err(&stderr)
        );
    }
    Ok(models)
}

fn truncate_for_err(text: &str) -> String {
    const MAX: usize = 500;
    let trimmed = text.trim();
    if trimmed.len() <= MAX {
        trimmed.to_string()
    } else {
        format!("{}…", &trimmed[..MAX])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_models_lines() {
        let stdout = "\
auto - Auto (current, default)
composer-2.5 - Composer 2.5
gpt-5.5-high - GPT-5.5 1M High
";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "Auto (current, default)");
        assert_eq!(models[1].id, "composer-2.5");
        assert_eq!(models[2].id, "gpt-5.5-high");
    }

    #[test]
    fn skips_headers_and_blank_lines() {
        let stdout = "Available models\n\nauto - Auto\n";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "auto");
    }

    #[test]
    fn parses_ansi_colorized_lines() {
        let stdout = "\u{1b}[1mauto\u{1b}[0m - \u{1b}[32mAuto (default)\u{1b}[0m\n";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "Auto (default)");
    }
}
