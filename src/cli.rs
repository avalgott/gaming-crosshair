use clap::Parser;

/// Click-through crosshair overlay for Linux.
#[derive(Parser, Debug)]
#[command(
    name = "crosshair",
    version,
    about = "Click-through crosshair overlay for Linux",
    arg_required_else_help = true
)]
pub struct Cli {
    /// Start the crosshair overlay
    #[arg(long, conflicts_with = "stop")]
    pub start: bool,

    /// Stop a running crosshair instance
    #[arg(long)]
    pub stop: bool,

    /// Open the calibration panel to move the dot live
    #[arg(long, conflicts_with_all = ["start", "stop"])]
    pub calibrate: bool,
}
