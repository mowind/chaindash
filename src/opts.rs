use clap::Parser;
use num_rational::Ratio;

fn parse_positive_interval(value: &str) -> Result<Ratio<u64>, String> {
    let interval = value.parse::<Ratio<u64>>().map_err(|err| err.to_string())?;
    if interval == Ratio::from_integer(0) {
        return Err("interval must be greater than 0".to_string());
    }

    Ok(interval)
}

fn parse_positive_u64(value: &str) -> Result<u64, String> {
    let parsed = value.parse::<u64>().map_err(|err| err.to_string())?;
    if parsed == 0 {
        return Err("value must be greater than 0".to_string());
    }

    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramNotifyEvent {
    All,
    Connection,
    ConnectionFailed,
    ConnectionRecovered,
    Ranking,
    RankingChanged,
    Daily,
    DailySummary,
}

fn parse_telegram_notify_event(value: &str) -> Result<TelegramNotifyEvent, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(TelegramNotifyEvent::All),
        "connection" => Ok(TelegramNotifyEvent::Connection),
        "connection-failed" | "connection_failed" => Ok(TelegramNotifyEvent::ConnectionFailed),
        "connection-recovered" | "connection_recovered" => {
            Ok(TelegramNotifyEvent::ConnectionRecovered)
        },
        "ranking" => Ok(TelegramNotifyEvent::Ranking),
        "ranking-changed" | "ranking_changed" => Ok(TelegramNotifyEvent::RankingChanged),
        "daily" => Ok(TelegramNotifyEvent::Daily),
        "daily-summary" | "daily_summary" => Ok(TelegramNotifyEvent::DailySummary),
        _ => Err(format!(
            "invalid telegram notify event: {value}. valid values: all, connection, \
             connection-failed, connection-recovered, ranking, ranking-changed, daily, \
             daily-summary"
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelegramQuietHours {
    start_minutes: u16,
    end_minutes: u16,
}

impl TelegramQuietHours {
    pub fn contains_minutes_since_midnight(
        self,
        minutes_since_midnight: u16,
    ) -> bool {
        if self.start_minutes < self.end_minutes {
            minutes_since_midnight >= self.start_minutes
                && minutes_since_midnight < self.end_minutes
        } else {
            minutes_since_midnight >= self.start_minutes
                || minutes_since_midnight < self.end_minutes
        }
    }
}

fn parse_telegram_time_of_day(value: &str) -> Result<u16, String> {
    let Some((hour, minute)) = value.trim().split_once(':') else {
        return Err(format!("invalid time-of-day: {value}. expected HH:MM format, e.g. 23:00"));
    };

    let hour = hour.parse::<u16>().map_err(|err| err.to_string())?;
    let minute = minute.parse::<u16>().map_err(|err| err.to_string())?;

    if hour >= 24 || minute >= 60 {
        return Err(format!(
            "invalid time-of-day: {value}. hour must be 0-23 and minute must be 0-59"
        ));
    }

    Ok(hour * 60 + minute)
}

fn parse_telegram_quiet_hours(value: &str) -> Result<TelegramQuietHours, String> {
    let Some((start, end)) = value.trim().split_once('-') else {
        return Err(format!("invalid telegram quiet hours: {value}. expected HH:MM-HH:MM format"));
    };

    let start_minutes = parse_telegram_time_of_day(start)?;
    let end_minutes = parse_telegram_time_of_day(end)?;

    if start_minutes == end_minutes {
        return Err("telegram quiet hours must have different start and end times".to_string());
    }

    Ok(TelegramQuietHours {
        start_minutes,
        end_minutes,
    })
}

#[derive(Parser, Debug)]
pub struct Opts {
    /// The platon connection endpoints, separated by `,`.
    #[arg(long, default_value = "test@ws://127.0.0.1:6789")]
    pub url: String,

    /// Render interval
    #[arg(long, default_value = "1", value_parser = parse_positive_interval)]
    pub interval: Ratio<u64>,

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
    #[arg(long, default_value = "2", value_parser = parse_positive_u64)]
    pub disk_refresh_interval: u64,

    /// Node IDs to show details for (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub node_id: Vec<String>,

    /// PlatON Explorer API base URL
    #[arg(long, default_value = "https://scan.platon.network/browser-server")]
    pub explorer_api_url: String,

    /// Telegram bot token used for notifications
    #[arg(long)]
    pub telegram_bot_token: Option<String>,

    /// Telegram chat IDs used for notifications (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub telegram_chat_id: Vec<String>,

    /// Telegram notification events (comma-separated)
    #[arg(long, value_delimiter = ',', value_parser = parse_telegram_notify_event)]
    pub telegram_notify_events: Vec<TelegramNotifyEvent>,

    /// Telegram quiet hours in local time, format HH:MM-HH:MM
    #[arg(long, value_parser = parse_telegram_quiet_hours)]
    pub telegram_quiet_hours: Option<TelegramQuietHours>,

    /// Minimum interval between repeated Telegram notifications for the same event key
    #[arg(long, default_value = "0")]
    pub telegram_rate_limit_seconds: u64,

    /// Template for connection-failed notifications
    #[arg(long)]
    pub telegram_template_connection_failed: Option<String>,

    /// Template for connection-recovered notifications
    #[arg(long)]
    pub telegram_template_connection_recovered: Option<String>,

    /// Template for ranking-changed notifications
    #[arg(long)]
    pub telegram_template_ranking_changed: Option<String>,

    /// Template for quiet-summary notifications
    #[arg(long)]
    pub telegram_template_quiet_summary: Option<String>,

    /// Template for daily-summary notifications
    #[arg(long)]
    pub telegram_template_daily_summary: Option<String>,

    /// Telegram Bot API base URL
    #[arg(long, default_value = "https://api.telegram.org")]
    pub telegram_api_url: String,
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

    #[test]
    fn test_zero_disk_refresh_interval_is_rejected() {
        let result = Opts::try_parse_from(["test", "--disk-refresh-interval", "0"]);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("value must be greater than 0"));
    }

    #[test]
    fn test_positive_disk_refresh_interval_is_accepted() {
        let opts = Opts::parse_from(["test", "--disk-refresh-interval", "5"]);

        assert_eq!(opts.disk_refresh_interval, 5);
    }

    #[test]
    fn test_multiple_node_ids_are_accepted() {
        let opts = Opts::parse_from(["test", "--node-id", "node-a,node-b"]);

        assert_eq!(opts.node_id, vec!["node-a".to_string(), "node-b".to_string()]);
    }

    #[test]
    fn test_telegram_options_are_accepted() {
        let opts = Opts::parse_from([
            "test",
            "--telegram-bot-token",
            "bot-token",
            "--telegram-chat-id",
            "123456,789012",
            "--telegram-notify-events",
            "connection,ranking-changed",
            "--telegram-quiet-hours",
            "23:00-08:00",
            "--telegram-rate-limit-seconds",
            "120",
            "--telegram-template-connection-failed",
            "{prefix} {node} failed: {reason}",
            "--telegram-template-connection-recovered",
            "{prefix} {node} recovered",
            "--telegram-template-ranking-changed",
            "{prefix} {node}: {previous}->{current}",
            "--telegram-template-quiet-summary",
            "{prefix} summary {count}\\n{details}",
            "--telegram-template-daily-summary",
            "{prefix} daily {date}\\n{details}",
        ]);

        assert_eq!(opts.telegram_bot_token.as_deref(), Some("bot-token"));
        assert_eq!(opts.telegram_chat_id, vec!["123456".to_string(), "789012".to_string()]);
        assert_eq!(
            opts.telegram_notify_events,
            vec![TelegramNotifyEvent::Connection, TelegramNotifyEvent::RankingChanged]
        );
        assert_eq!(
            opts.telegram_quiet_hours,
            Some(TelegramQuietHours {
                start_minutes: 23 * 60,
                end_minutes: 8 * 60,
            })
        );
        assert_eq!(opts.telegram_rate_limit_seconds, 120);
        assert_eq!(
            opts.telegram_template_connection_failed.as_deref(),
            Some("{prefix} {node} failed: {reason}")
        );
        assert_eq!(
            opts.telegram_template_connection_recovered.as_deref(),
            Some("{prefix} {node} recovered")
        );
        assert_eq!(
            opts.telegram_template_ranking_changed.as_deref(),
            Some("{prefix} {node}: {previous}->{current}")
        );
        assert_eq!(
            opts.telegram_template_quiet_summary.as_deref(),
            Some("{prefix} summary {count}\\n{details}")
        );
        assert_eq!(
            opts.telegram_template_daily_summary.as_deref(),
            Some("{prefix} daily {date}\\n{details}")
        );
        assert_eq!(opts.telegram_api_url, "https://api.telegram.org");
    }

    #[test]
    fn test_invalid_telegram_notify_event_is_rejected() {
        let result =
            Opts::try_parse_from(["test", "--telegram-notify-events", "not-a-valid-event"]);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid telegram notify event"));
    }

    #[test]
    fn test_telegram_quiet_hours_contains_overnight_range() {
        let quiet_hours =
            parse_telegram_quiet_hours("23:00-08:00").expect("quiet hours should parse");

        assert!(quiet_hours.contains_minutes_since_midnight(23 * 60));
        assert!(quiet_hours.contains_minutes_since_midnight(7 * 60 + 59));
        assert!(!quiet_hours.contains_minutes_since_midnight(8 * 60));
        assert!(!quiet_hours.contains_minutes_since_midnight(22 * 60 + 59));
    }

    #[test]
    fn test_invalid_telegram_quiet_hours_are_rejected() {
        let result = Opts::try_parse_from(["test", "--telegram-quiet-hours", "23:00/08:00"]);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid telegram quiet hours"));
    }
}
