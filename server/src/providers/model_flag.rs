//! A saved model id, or nothing when the agent leaves the choice to the CLI.

/// `None` for a missing, empty, or whitespace-only model.
pub fn selected_model(model: Option<&str>) -> Option<&str> {
    match model.map(str::trim) {
        Some(model) if !model.is_empty() => Some(model),
        _ => None,
    }
}

/// Append `flag` and the model id when one is selected.
pub fn push_model_flag(args: &mut Vec<String>, flag: &str, model: Option<&str>) {
    if let Some(model) = selected_model(model) {
        args.push(flag.to_string());
        args.push(model.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_model_pushes_no_flag() {
        for model in [None, Some(""), Some("   ")] {
            let mut args = vec!["run".to_string()];
            push_model_flag(&mut args, "--model", model);
            push_model_flag(&mut args, "-m", model);
            assert_eq!(args, ["run"]);
        }
    }

    #[test]
    fn selected_model_pushes_its_flag() {
        let mut args = Vec::new();
        push_model_flag(&mut args, "--model", Some("  sonnet  "));
        assert_eq!(args, ["--model", "sonnet"]);
    }
}
