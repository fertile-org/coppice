//! Future CliTmuxProvider backend for Claude Code / Codex.
//! See docs/providers/ (claude-code.md, codex.md).

pub struct TmuxStream;

impl TmuxStream {
    pub fn not_implemented() -> ! {
        unimplemented!("CliTmuxProvider is documented for post-M04")
    }
}
