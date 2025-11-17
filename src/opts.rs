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
}
