use clap::Parser;
use num_rational::Ratio;

#[derive(Parser, Debug)]
pub struct Opts {
    /// The platon connection endpoints, separated by `,`.
    #[arg(long, default_value = "test@http://127.0.0.1:6789")]
    pub url: String,

    /// Render interval
    #[arg(long, default_value = "1")]
    pub interval: Ratio<u64>,

    /// Enable docker stats
    #[arg(long)]
    pub enable_docker_stats: bool,

    /// Docker service port
    #[arg(long, default_value = "2375")]
    pub docker_port: u16,

    /// Enable debug log
    #[arg(long)]
    pub debug: bool,

    /// Disk mount points to monitor (comma-separated)
    #[arg(long, value_delimiter = ',', default_value = "/,/opt")]
    pub disk_mount_points: Vec<String>,

    /// Enable automatic disk mount point discovery
    #[arg(long, default_value = "false")]
    pub disk_auto_discovery: bool,

    /// Disk alert threshold percentage (default: 90%)
    #[arg(long, default_value = "90.0")]
    pub disk_alert_threshold: f32,

    /// Disk refresh interval in seconds (default: 2)
    #[arg(long, default_value = "2")]
    pub disk_refresh_interval: u64,

    /// Node ID to show details for
    #[arg(long)]
    pub node_id: Option<String>,
}
