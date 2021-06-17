use clap::Clap;
use num_rational::Ratio;

#[derive(Clap)]
#[clap(version = "1.0", author = "mowind <wjinwen.1988@gmail.com>")]
pub struct Opts {
    /// The platon connection endpoints, separated by `,`.
    #[clap(long, default_value = "test@http://127.0.0.1:6789")]
    pub url: String,

    /// Render interval
    #[clap(long, default_value = "1")]
    pub interval: Ratio<u64>,

    /// Enable docker stats
    #[clap(long)]
    pub enable_docker_stats: bool,

    /// Docker service port
    #[clap(long, default_value = "2375")]
    pub docker_port: u16,

    /// Enable debug log
    #[clap(long)]
    pub debug: bool,

    /// Ledger name
    #[clap(long)]
    pub ledger_name: String,
}
