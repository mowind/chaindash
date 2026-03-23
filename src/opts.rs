use clap::Parser;
use num_rational::Ratio;

fn parse_positive_interval(value: &str) -> Result<Ratio<u64>, String> {
    let interval = value.parse::<Ratio<u64>>().map_err(|err| err.to_string())?;
    if interval == Ratio::from_integer(0) {
        return Err("interval must be greater than 0".to_string());
    }

    Ok(interval)
}

#[derive(Parser, Debug)]
pub struct Opts {
    /// The platon connection endpoints, separated by `,`.
    #[arg(long, default_value = "test@ws://127.0.0.1:6789")]
    pub url: String,

    /// Render interval
    #[arg(long, default_value = "1", value_parser = parse_positive_interval)]
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

    /// PlatON Explorer API base URL
    #[arg(long, default_value = "https://scan.platon.network/browser-server")]
    pub explorer_api_url: String,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn test_default_url_uses_websocket_scheme() {
        let opts = Opts::parse_from(["test"]);

        assert_eq!(opts.url, "test@ws://127.0.0.1:6789");
    }

    #[test]
    fn test_zero_interval_is_rejected() {
        let result = Opts::try_parse_from(["test", "--interval", "0"]);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("interval must be greater than 0"));
    }

    #[test]
    fn test_positive_interval_is_accepted() {
        let opts = Opts::parse_from(["test", "--interval", "5"]);

        assert_eq!(opts.interval, Ratio::from_integer(5));
    }
}
