use anyhow::Result;

use niiterm::cli::Args;

fn main() -> Result<()> {
    let args = Args::parse_args();
    args.init_tracing()?;

    if args.interactive {
        niiterm::tui::run(args)
    } else {
        niiterm::oneshot::run(args)
    }
}
