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
    #[arg(long, conflicts_with_all = ["stop", "update"])]
    pub start: bool,

    /// Stop a running crosshair instance
    #[arg(long, conflicts_with_all = ["start", "update"])]
    pub stop: bool,

    /// Open the calibration panel to move the dot live
    #[arg(long, conflicts_with_all = ["start", "stop", "update"])]
    pub calibrate: bool,

    /// Update crosshair to the latest GitHub release
    #[arg(long, conflicts_with_all = ["start", "stop", "calibrate"])]
    pub update: bool,
}
