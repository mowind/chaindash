use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
    time::{
        Duration,
        Instant,
    },
};

use chrono::{
    Local,
    Timelike,
};
use log::{
    debug,
    warn,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    error::{
        ChaindashError,
        Result,
    },
    opts::{
        Opts,
        TelegramNotifyEvent,
        TelegramQuietHours,
    },
};

const TELEGRAM_MESSAGE_PREFIX: &str = "[chaindash]";
const DEFAULT_CONNECTION_FAILED_TEMPLATE: &str = "{prefix} ❌ {node} 连接失败：{reason}";
const DEFAULT_CONNECTION_RECOVERED_TEMPLATE: &str = "{prefix} ✅ {node} 连接恢复";
const DEFAULT_RANKING_CHANGED_TEMPLATE: &str =
    "{prefix} {icon} {node} 排名 {previous} → {current}（{delta_text}）";
const DEFAULT_QUIET_SUMMARY_TEMPLATE: &str = "{prefix} 🌙 静默期摘要（共 {count} 条）\n{details}";
const QUIET_SUMMARY_PREVIEW_LIMIT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationEventKind {
    ConnectionFailed,
    ConnectionRecovered,
    RankingChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TelegramNotificationFilter {
    connection_failed: bool,
    connection_recovered: bool,
    ranking_changed: bool,
}

impl TelegramNotificationFilter {
    fn all() -> Self {
        Self {
            connection_failed: true,
            connection_recovered: true,
            ranking_changed: true,
        }
    }

    fn none() -> Self {
        Self {
            connection_failed: false,
            connection_recovered: false,
            ranking_changed: false,
        }
    }

    fn from_opts(events: &[TelegramNotifyEvent]) -> Self {
        if events.is_empty() {
            return Self::all();
        }

        let mut filter = Self::none();
        for event in events {
            match event {
                TelegramNotifyEvent::All => filter = Self::all(),
                TelegramNotifyEvent::Connection => {
                    filter.connection_failed = true;
                    filter.connection_recovered = true;
                },
                TelegramNotifyEvent::ConnectionFailed => filter.connection_failed = true,
                TelegramNotifyEvent::ConnectionRecovered => filter.connection_recovered = true,
                TelegramNotifyEvent::Ranking | TelegramNotifyEvent::RankingChanged => {
                    filter.ranking_changed = true;
                },
            }
        }

        filter
    }

    fn allows(
        self,
        event: NotificationEventKind,
    ) -> bool {
        match event {
            NotificationEventKind::ConnectionFailed => self.connection_failed,
            NotificationEventKind::ConnectionRecovered => self.connection_recovered,
            NotificationEventKind::RankingChanged => self.ranking_changed,
        }
    }
}

#[derive(Debug, Clone)]
struct TelegramTemplates {
    connection_failed: String,
    connection_recovered: String,
    ranking_changed: String,
    quiet_summary: String,
}

impl TelegramTemplates {
    fn from_opts(opts: &Opts) -> Self {
        Self {
            connection_failed: normalize_template(
                opts.telegram_template_connection_failed
                    .as_deref()
                    .unwrap_or(DEFAULT_CONNECTION_FAILED_TEMPLATE),
            ),
            connection_recovered: normalize_template(
                opts.telegram_template_connection_recovered
                    .as_deref()
                    .unwrap_or(DEFAULT_CONNECTION_RECOVERED_TEMPLATE),
            ),
            ranking_changed: normalize_template(
                opts.telegram_template_ranking_changed
                    .as_deref()
                    .unwrap_or(DEFAULT_RANKING_CHANGED_TEMPLATE),
            ),
            quiet_summary: normalize_template(
                opts.telegram_template_quiet_summary
                    .as_deref()
                    .unwrap_or(DEFAULT_QUIET_SUMMARY_TEMPLATE),
            ),
        }
    }
}

#[derive(Debug, Clone)]
struct TelegramConfig {
    bot_token: String,
    chat_ids: Vec<String>,
    enabled_events: TelegramNotificationFilter,
    quiet_hours: Option<TelegramQuietHours>,
    rate_limit: Duration,
    templates: TelegramTemplates,
    api_url: String,
}

impl TelegramConfig {
    fn from_opts(opts: &Opts) -> Result<Option<Self>> {
        let bot_token = trimmed_option(opts.telegram_bot_token.as_deref());
        let chat_ids = trimmed_values(&opts.telegram_chat_id);

        match (bot_token, chat_ids.is_empty()) {
            (None, true) => Ok(None),
            (Some(_), true) => Err(ChaindashError::Other(
                "at least one telegram chat id is required when telegram bot token is set"
                    .to_string(),
            )),
            (None, false) => Err(ChaindashError::Other(
                "telegram bot token is required when telegram chat id is set".to_string(),
            )),
            (Some(bot_token), false) => {
                let api_url = opts.telegram_api_url.trim().trim_end_matches('/').to_string();
                if api_url.is_empty() {
                    return Err(ChaindashError::Other(
                        "telegram api url cannot be empty when telegram notifications are enabled"
                            .to_string(),
                    ));
                }

                Ok(Some(Self {
                    bot_token: bot_token.to_string(),
                    chat_ids,
                    enabled_events: TelegramNotificationFilter::from_opts(
                        &opts.telegram_notify_events,
                    ),
                    quiet_hours: opts.telegram_quiet_hours,
                    rate_limit: Duration::from_secs(opts.telegram_rate_limit_seconds),
                    templates: TelegramTemplates::from_opts(opts),
                    api_url,
                }))
            },
        }
    }

    fn send_message_url(&self) -> String {
        format!("{}/bot{}/sendMessage", self.api_url, self.bot_token)
    }

    fn is_quiet_time_now(&self) -> bool {
        let Some(quiet_hours) = self.quiet_hours else {
            return false;
        };

        let now = Local::now();
        let minutes_since_midnight = (now.hour() * 60 + now.minute()) as u16;

        quiet_hours.contains_minutes_since_midnight(minutes_since_midnight)
    }

    #[cfg(test)]
    fn is_quiet_time_at(
        &self,
        minutes_since_midnight: u16,
    ) -> bool {
        self.quiet_hours.is_some_and(|quiet_hours| {
            quiet_hours.contains_minutes_since_midnight(minutes_since_midnight)
        })
    }
}

fn trimmed_option(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn trimmed_values(values: &[String]) -> Vec<String> {
    let mut trimmed = Vec::new();

    for value in values {
        let value = value.trim();
        if value.is_empty() || trimmed.iter().any(|existing| existing == value) {
            continue;
        }

        trimmed.push(value.to_string());
    }

    trimmed
}

fn normalize_template(template: &str) -> String {
    let mut normalized = String::with_capacity(template.len());
    let mut chars = template.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            normalized.push(ch);
            continue;
        }

        match chars.next() {
            Some('n') => normalized.push('\n'),
            Some('r') => normalized.push('\r'),
            Some('t') => normalized.push('\t'),
            Some('\\') => normalized.push('\\'),
            Some(other) => {
                normalized.push('\\');
                normalized.push(other);
            },
            None => normalized.push('\\'),
        }
    }

    normalized
}

fn display_node_name(node_name: &str) -> &str {
    if node_name.trim().is_empty() {
        "未命名节点"
    } else {
        node_name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Healthy,
    Unhealthy,
}

#[derive(Debug, Default)]
struct QuietSummaryBucket {
    count: usize,
    names: Vec<String>,
}

impl QuietSummaryBucket {
    fn record(
        &mut self,
        name: &str,
    ) {
        self.count += 1;

        if self.names.len() >= QUIET_SUMMARY_PREVIEW_LIMIT
            || self.names.iter().any(|existing| existing == name)
        {
            return;
        }

        self.names.push(name.to_string());
    }

    fn render_line(
        &self,
        label: &str,
    ) -> Option<String> {
        if self.count == 0 {
            return None;
        }

        if self.names.is_empty() {
            return Some(format!("{label} {} 次", self.count));
        }

        Some(format!("{label} {} 次：{}", self.count, self.names.join(", ")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuietSummarySnapshot {
    total_count: usize,
    details: String,
}

#[derive(Debug, Default)]
struct QuietSummaryBuffer {
    total_count: usize,
    connection_failed: QuietSummaryBucket,
    connection_recovered: QuietSummaryBucket,
    ranking_changed: QuietSummaryBucket,
}

impl QuietSummaryBuffer {
    fn record(
        &mut self,
        event: NotificationEventKind,
        subject: &str,
    ) {
        self.total_count += 1;

        match event {
            NotificationEventKind::ConnectionFailed => self.connection_failed.record(subject),
            NotificationEventKind::ConnectionRecovered => self.connection_recovered.record(subject),
            NotificationEventKind::RankingChanged => self.ranking_changed.record(subject),
        }
    }

    fn take_snapshot(&mut self) -> Option<QuietSummarySnapshot> {
        let snapshot = std::mem::take(self);
        snapshot.render_snapshot()
    }

    fn render_snapshot(self) -> Option<QuietSummarySnapshot> {
        if self.total_count == 0 {
            return None;
        }

        let mut lines = Vec::new();

        if let Some(line) = self.connection_failed.render_line("连接失败") {
            lines.push(line);
        }
        if let Some(line) = self.connection_recovered.render_line("连接恢复") {
            lines.push(line);
        }
        if let Some(line) = self.ranking_changed.render_line("排名变化") {
            lines.push(line);
        }

        Some(QuietSummarySnapshot {
            total_count: self.total_count,
            details: lines.join("\n"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RankingChange {
    previous: i32,
    current: i32,
}

#[derive(Debug, Default)]
struct NotificationState {
    connection_states: HashMap<String, ConnectionState>,
    last_rankings: HashMap<String, i32>,
    last_sent_at: HashMap<String, Instant>,
    quiet_summary: QuietSummaryBuffer,
}

impl NotificationState {
    fn mark_connection_failed(
        &mut self,
        key: &str,
    ) -> bool {
        !matches!(
            self.connection_states.insert(key.to_string(), ConnectionState::Unhealthy),
            Some(ConnectionState::Unhealthy)
        )
    }

    fn mark_connection_recovered(
        &mut self,
        key: &str,
    ) -> bool {
        matches!(
            self.connection_states.insert(key.to_string(), ConnectionState::Healthy),
            Some(ConnectionState::Unhealthy)
        )
    }

    fn plan_ranking_change(
        &mut self,
        node_id: &str,
        ranking: i32,
    ) -> Option<RankingChange> {
        if ranking <= 0 {
            return None;
        }

        let previous = self
            .last_rankings
            .insert(node_id.to_string(), ranking)
            .filter(|previous| *previous > 0)?;

        if previous == ranking {
            return None;
        }

        Some(RankingChange {
            previous,
            current: ranking,
        })
    }

    fn allow_delivery(
        &mut self,
        rate_limit_key: &str,
        now: Instant,
        rate_limit: Duration,
    ) -> bool {
        if rate_limit.is_zero() {
            return true;
        }

        if self
            .last_sent_at
            .get(rate_limit_key)
            .is_some_and(|last_sent_at| now.saturating_duration_since(*last_sent_at) < rate_limit)
        {
            return false;
        }

        self.last_sent_at.insert(rate_limit_key.to_string(), now);
        true
    }

    fn record_quiet_summary(
        &mut self,
        event: NotificationEventKind,
        subject: &str,
    ) {
        self.quiet_summary.record(event, subject);
    }

    fn take_quiet_summary_snapshot(&mut self) -> Option<QuietSummarySnapshot> {
        self.quiet_summary.take_snapshot()
    }
}

#[derive(Debug, Serialize)]
struct TelegramSendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct TelegramSendMessageResponse {
    ok: bool,
    description: Option<String>,
}

#[derive(Debug)]
pub(crate) struct TelegramNotifier {
    client: reqwest::Client,
    config: TelegramConfig,
    state: Mutex<NotificationState>,
}

#[derive(Debug)]
enum SendDecision {
    SuppressedByRateLimit,
    SuppressedByQuietHours,
    Send { quiet_summary: Option<QuietSummarySnapshot> },
}

impl TelegramNotifier {
    pub(crate) fn from_opts(opts: &Opts) -> Result<Option<Arc<Self>>> {
        let Some(config) = TelegramConfig::from_opts(opts)? else {
            return Ok(None);
        };

        Ok(Some(Arc::new(Self {
            client: reqwest::Client::new(),
            config,
            state: Mutex::new(NotificationState::default()),
        })))
    }

    pub(crate) async fn notify_node_connection_failed(
        &self,
        node_name: &str,
        node_url: &str,
        reason: &str,
    ) {
        let key = Self::connection_key(node_name, node_url);
        let should_notify = {
            let mut state = self.state.lock().expect("telegram notifier mutex poisoned");
            state.mark_connection_failed(&key)
        };

        if !should_notify {
            return;
        }

        self.send_if_enabled(
            NotificationEventKind::ConnectionFailed,
            &format!("connection-failed:{key}"),
            display_node_name(node_name),
            self.render_connection_failed_message(node_name, reason),
        )
        .await;
    }

    pub(crate) async fn notify_node_connection_recovered(
        &self,
        node_name: &str,
        node_url: &str,
    ) {
        let key = Self::connection_key(node_name, node_url);
        let should_notify = {
            let mut state = self.state.lock().expect("telegram notifier mutex poisoned");
            state.mark_connection_recovered(&key)
        };

        if !should_notify {
            return;
        }

        self.send_if_enabled(
            NotificationEventKind::ConnectionRecovered,
            &format!("connection-recovered:{key}"),
            display_node_name(node_name),
            self.render_connection_recovered_message(node_name),
        )
        .await;
    }

    pub(crate) async fn notify_node_ranking_change(
        &self,
        node_id: &str,
        node_name: &str,
        ranking: i32,
    ) {
        let change = {
            let mut state = self.state.lock().expect("telegram notifier mutex poisoned");
            state.plan_ranking_change(node_id, ranking)
        };
        let Some(change) = change else {
            return;
        };

        self.send_if_enabled(
            NotificationEventKind::RankingChanged,
            &format!("ranking-changed:{node_id}"),
            display_node_name(node_name),
            self.render_ranking_changed_message(node_name, change),
        )
        .await;
    }

    fn connection_key(
        node_name: &str,
        node_url: &str,
    ) -> String {
        format!("{node_name}@{node_url}")
    }

    fn render_connection_failed_message(
        &self,
        node_name: &str,
        reason: &str,
    ) -> String {
        render_template(
            &self.config.templates.connection_failed,
            &[
                ("prefix", TELEGRAM_MESSAGE_PREFIX),
                ("node", display_node_name(node_name)),
                ("reason", reason),
            ],
        )
    }

    fn render_connection_recovered_message(
        &self,
        node_name: &str,
    ) -> String {
        render_template(
            &self.config.templates.connection_recovered,
            &[("prefix", TELEGRAM_MESSAGE_PREFIX), ("node", display_node_name(node_name))],
        )
    }

    fn render_ranking_changed_message(
        &self,
        node_name: &str,
        change: RankingChange,
    ) -> String {
        let delta = change.previous.abs_diff(change.current).to_string();
        let (icon, direction, delta_text) = if change.current < change.previous {
            ("📈", "up", format!("+{}", change.previous.abs_diff(change.current)))
        } else {
            ("📉", "down", format!("-{}", change.previous.abs_diff(change.current)))
        };
        let previous = change.previous.to_string();
        let current = change.current.to_string();

        render_template(
            &self.config.templates.ranking_changed,
            &[
                ("prefix", TELEGRAM_MESSAGE_PREFIX),
                ("icon", icon),
                ("node", display_node_name(node_name)),
                ("previous", previous.as_str()),
                ("current", current.as_str()),
                ("delta", delta.as_str()),
                ("delta_text", delta_text.as_str()),
                ("direction", direction),
            ],
        )
    }

    fn render_quiet_summary_message(
        &self,
        summary: &QuietSummarySnapshot,
    ) -> String {
        let count = summary.total_count.to_string();
        render_template(
            &self.config.templates.quiet_summary,
            &[
                ("prefix", TELEGRAM_MESSAGE_PREFIX),
                ("count", count.as_str()),
                ("details", summary.details.as_str()),
            ],
        )
    }

    async fn send_if_enabled(
        &self,
        event: NotificationEventKind,
        rate_limit_key: &str,
        summary_subject: &str,
        message: String,
    ) {
        if !self.config.enabled_events.allows(event) {
            return;
        }

        let quiet_time_now = self.config.is_quiet_time_now();
        let decision = {
            let mut state = self.state.lock().expect("telegram notifier mutex poisoned");
            if !state.allow_delivery(rate_limit_key, Instant::now(), self.config.rate_limit) {
                SendDecision::SuppressedByRateLimit
            } else if quiet_time_now {
                state.record_quiet_summary(event, summary_subject);
                SendDecision::SuppressedByQuietHours
            } else {
                SendDecision::Send {
                    quiet_summary: state.take_quiet_summary_snapshot(),
                }
            }
        };

        match decision {
            SendDecision::SuppressedByRateLimit => {
                debug!("telegram notification suppressed by rate limit: {rate_limit_key}");
            },
            SendDecision::SuppressedByQuietHours => {
                debug!("telegram notification buffered by quiet hours: {rate_limit_key}");
            },
            SendDecision::Send { quiet_summary } => {
                if let Some(quiet_summary) = quiet_summary {
                    let quiet_summary = self.render_quiet_summary_message(&quiet_summary);
                    self.send_message(&quiet_summary).await;
                }
                self.send_message(&message).await;
            },
        }
    }

    async fn send_message(
        &self,
        text: &str,
    ) {
        for chat_id in &self.config.chat_ids {
            self.send_message_to_chat(chat_id, text).await;
        }
    }

    async fn send_message_to_chat(
        &self,
        chat_id: &str,
        text: &str,
    ) {
        let request = TelegramSendMessageRequest { chat_id, text };

        match self.client.post(self.config.send_message_url()).json(&request).send().await {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    let response_text = response.text().await.unwrap_or_default();
                    if let Ok(body) =
                        serde_json::from_str::<TelegramSendMessageResponse>(&response_text)
                    {
                        warn!(
                            "Telegram sendMessage 请求失败，chat_id {}，HTTP {}：{}",
                            chat_id,
                            status,
                            describe_telegram_error(
                                body.description.as_deref().unwrap_or(response_text.as_str())
                            )
                        );
                    } else if response_text.trim().is_empty() {
                        warn!(
                            "Telegram sendMessage 请求失败，chat_id {}，HTTP {}",
                            chat_id, status
                        );
                    } else {
                        warn!(
                            "Telegram sendMessage 请求失败，chat_id {}，HTTP {}：{}",
                            chat_id,
                            status,
                            describe_telegram_error(&response_text)
                        );
                    }
                    return;
                }

                match response.json::<TelegramSendMessageResponse>().await {
                    Ok(body) if body.ok => {},
                    Ok(body) => {
                        warn!(
                            "Telegram sendMessage API 返回 ok=false，chat_id {}：{}",
                            chat_id,
                            describe_telegram_error(
                                body.description.as_deref().unwrap_or("unknown error")
                            )
                        );
                    },
                    Err(err) => {
                        warn!("解析 Telegram sendMessage 响应失败，chat_id {}：{}", chat_id, err);
                    },
                }
            },
            Err(err) => {
                warn!("发送 Telegram 消息失败，chat_id {}：{}", chat_id, err);
            },
        }
    }
}

fn render_template(
    template: &str,
    replacements: &[(&str, &str)],
) -> String {
    let mut rendered = template.to_string();

    for (key, value) in replacements {
        rendered = rendered.replace(&format!("{{{key}}}"), value);
    }

    rendered
}

fn describe_telegram_error(description: &str) -> String {
    let trimmed = description.trim();
    let normalized = trimmed.to_ascii_lowercase();

    let translated = if normalized.contains("chat not found") {
        "chat_id 无效，或目标会话不存在 / bot 不在该会话中"
    } else if normalized.contains("bot can't initiate conversation with a user") {
        "Bot 不能主动向用户发起会话，请先给 bot 发送 /start"
    } else if normalized.contains("bot was blocked by the user") {
        "Bot 已被目标用户拉黑"
    } else if normalized.contains("user is deactivated") {
        "目标用户已停用"
    } else if normalized.contains("chat is deactivated") {
        "目标会话已停用"
    } else if normalized.contains("group chat was upgraded to a supergroup chat") {
        "群组已升级为超级群，请更新为新的 chat_id"
    } else if normalized.contains("not enough rights to send text messages to the chat") {
        "Bot 在目标会话中没有发送文本消息的权限"
    } else if normalized.contains("have no rights to send a message") {
        "Bot 在目标会话中没有发送消息的权限"
    } else {
        return trimmed.to_string();
    };

    format!("{}（原始错误: {}）", translated, trimmed)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    fn create_test_notifier_with_templates(templates: TelegramTemplates) -> TelegramNotifier {
        TelegramNotifier {
            client: reqwest::Client::new(),
            config: TelegramConfig {
                bot_token: "bot-token".to_string(),
                chat_ids: vec!["10001".to_string()],
                enabled_events: TelegramNotificationFilter::all(),
                quiet_hours: None,
                rate_limit: Duration::from_secs(0),
                templates,
                api_url: "https://api.telegram.org".to_string(),
            },
            state: Mutex::new(NotificationState::default()),
        }
    }

    fn create_test_notifier() -> TelegramNotifier {
        create_test_notifier_with_templates(TelegramTemplates {
            connection_failed: DEFAULT_CONNECTION_FAILED_TEMPLATE.to_string(),
            connection_recovered: DEFAULT_CONNECTION_RECOVERED_TEMPLATE.to_string(),
            ranking_changed: DEFAULT_RANKING_CHANGED_TEMPLATE.to_string(),
            quiet_summary: DEFAULT_QUIET_SUMMARY_TEMPLATE.to_string(),
        })
    }

    #[test]
    fn test_notification_filter_defaults_to_all_events() {
        let filter = TelegramNotificationFilter::from_opts(&[]);

        assert!(filter.allows(NotificationEventKind::ConnectionFailed));
        assert!(filter.allows(NotificationEventKind::ConnectionRecovered));
        assert!(filter.allows(NotificationEventKind::RankingChanged));
    }

    #[test]
    fn test_render_template_replaces_known_placeholders() {
        let rendered = render_template(
            "{prefix} {node} {current}",
            &[("prefix", "[chaindash]"), ("node", "验证节点A"), ("current", "5")],
        );

        assert_eq!(rendered, "[chaindash] 验证节点A 5");
    }

    #[test]
    fn test_normalize_template_unescapes_newline_sequences() {
        assert_eq!(normalize_template("a\\nb"), "a\nb");
    }

    #[test]
    fn test_describe_telegram_error_translates_common_chat_not_found_error() {
        let message = describe_telegram_error("Bad Request: chat not found");

        assert!(message.contains("chat_id 无效"));
        assert!(message.contains("chat not found"));
    }

    #[test]
    fn test_describe_telegram_error_preserves_unknown_errors() {
        let message = describe_telegram_error("some custom telegram error");

        assert_eq!(message, "some custom telegram error");
    }

    #[test]
    fn test_quiet_summary_buffer_formats_suppressed_notifications() {
        let mut buffer = QuietSummaryBuffer::default();
        buffer.record(NotificationEventKind::ConnectionFailed, "main");
        buffer.record(NotificationEventKind::ConnectionFailed, "backup");
        buffer.record(NotificationEventKind::RankingChanged, "验证节点A");

        let summary = buffer.take_snapshot().expect("summary should exist");

        assert_eq!(
            summary,
            QuietSummarySnapshot {
                total_count: 3,
                details: "连接失败 2 次：main, backup\n排名变化 1 次：验证节点A".to_string(),
            }
        );
        assert_eq!(buffer.take_snapshot(), None);
    }

    #[test]
    fn test_quiet_summary_bucket_limits_preview_names() {
        let mut bucket = QuietSummaryBucket::default();
        bucket.record("node-a");
        bucket.record("node-b");
        bucket.record("node-c");
        bucket.record("node-d");

        assert_eq!(bucket.count, 4);
        assert_eq!(bucket.names, vec!["node-a", "node-b", "node-c"]);
    }

    #[test]
    fn test_notification_filter_supports_event_groups() {
        let filter = TelegramNotificationFilter::from_opts(&[
            TelegramNotifyEvent::Connection,
            TelegramNotifyEvent::RankingChanged,
        ]);

        assert!(filter.allows(NotificationEventKind::ConnectionFailed));
        assert!(filter.allows(NotificationEventKind::ConnectionRecovered));
        assert!(filter.allows(NotificationEventKind::RankingChanged));
    }

    #[test]
    fn test_notification_filter_supports_single_event_selection() {
        let filter =
            TelegramNotificationFilter::from_opts(&[TelegramNotifyEvent::ConnectionRecovered]);

        assert!(!filter.allows(NotificationEventKind::ConnectionFailed));
        assert!(filter.allows(NotificationEventKind::ConnectionRecovered));
        assert!(!filter.allows(NotificationEventKind::RankingChanged));
    }

    #[test]
    fn test_telegram_config_accepts_multiple_chat_ids() {
        let opts = Opts::parse_from([
            "test",
            "--telegram-bot-token",
            "bot-token",
            "--telegram-chat-id",
            "10001,10002,10001",
            "--telegram-notify-events",
            "connection,ranking",
        ]);

        let config = TelegramConfig::from_opts(&opts)
            .expect("config should parse")
            .expect("telegram config should be enabled");

        assert_eq!(config.chat_ids, vec!["10001".to_string(), "10002".to_string()]);
        assert!(config.enabled_events.allows(NotificationEventKind::ConnectionFailed));
        assert!(config.enabled_events.allows(NotificationEventKind::ConnectionRecovered));
        assert!(config.enabled_events.allows(NotificationEventKind::RankingChanged));
    }

    #[test]
    fn test_telegram_templates_accept_custom_templates() {
        let opts = Opts::parse_from([
            "test",
            "--telegram-bot-token",
            "bot-token",
            "--telegram-chat-id",
            "10001",
            "--telegram-template-connection-failed",
            "{prefix} FAIL {node}: {reason}",
            "--telegram-template-connection-recovered",
            "{prefix} OK {node}",
            "--telegram-template-ranking-changed",
            "{prefix} {node} {previous}->{current} {delta_text}",
            "--telegram-template-quiet-summary",
            "{prefix} summary {count}\\n{details}",
        ]);

        let config = TelegramConfig::from_opts(&opts)
            .expect("config should parse")
            .expect("telegram config should be enabled");

        assert_eq!(config.templates.connection_failed, "{prefix} FAIL {node}: {reason}");
        assert_eq!(config.templates.connection_recovered, "{prefix} OK {node}");
        assert_eq!(
            config.templates.ranking_changed,
            "{prefix} {node} {previous}->{current} {delta_text}"
        );
        assert_eq!(config.templates.quiet_summary, "{prefix} summary {count}\n{details}");
    }

    #[test]
    fn test_custom_connection_failed_template_is_rendered() {
        let notifier = create_test_notifier_with_templates(TelegramTemplates {
            connection_failed: "{prefix} FAIL {node}: {reason}".to_string(),
            connection_recovered: DEFAULT_CONNECTION_RECOVERED_TEMPLATE.to_string(),
            ranking_changed: DEFAULT_RANKING_CHANGED_TEMPLATE.to_string(),
            quiet_summary: DEFAULT_QUIET_SUMMARY_TEMPLATE.to_string(),
        });

        let message = notifier.render_connection_failed_message("main", "rpc timeout");

        assert_eq!(message, "[chaindash] FAIL main: rpc timeout");
    }

    #[test]
    fn test_telegram_config_accepts_quiet_hours_and_rate_limit() {
        let opts = Opts::parse_from([
            "test",
            "--telegram-bot-token",
            "bot-token",
            "--telegram-chat-id",
            "10001",
            "--telegram-quiet-hours",
            "23:00-08:00",
            "--telegram-rate-limit-seconds",
            "120",
        ]);

        let config = TelegramConfig::from_opts(&opts)
            .expect("config should parse")
            .expect("telegram config should be enabled");

        assert_eq!(config.rate_limit, Duration::from_secs(120));
        assert!(config.is_quiet_time_at(23 * 60));
        assert!(config.is_quiet_time_at(7 * 60 + 59));
        assert!(!config.is_quiet_time_at(8 * 60));
    }

    #[test]
    fn test_connection_failure_is_only_reported_once_until_recovery() {
        let mut state = NotificationState::default();

        assert!(state.mark_connection_failed("main@ws://node"));
        assert!(!state.mark_connection_failed("main@ws://node"));
    }

    #[test]
    fn test_connection_recovery_requires_previous_failure() {
        let mut state = NotificationState::default();

        assert!(!state.mark_connection_recovered("main@ws://node"));

        state.mark_connection_failed("main@ws://node");

        assert!(state.mark_connection_recovered("main@ws://node"));
    }

    #[test]
    fn test_initial_ranking_observation_does_not_send_notification() {
        let mut state = NotificationState::default();

        let change = state.plan_ranking_change("node-a", 7);

        assert_eq!(change, None);
    }

    #[test]
    fn test_ranking_improvement_uses_upward_message() {
        let notifier = create_test_notifier();

        let message = notifier.render_ranking_changed_message(
            "验证节点A",
            RankingChange {
                previous: 7,
                current: 5,
            },
        );

        assert_eq!(message, "[chaindash] 📈 验证节点A 排名 7 → 5（+2）");
        assert!(!message.contains("Node ID"));
        assert!(!message.contains("节点:"));
    }

    #[test]
    fn test_ranking_decline_uses_downward_message() {
        let notifier = create_test_notifier();

        let message = notifier.render_ranking_changed_message(
            "验证节点A",
            RankingChange {
                previous: 5,
                current: 8,
            },
        );

        assert_eq!(message, "[chaindash] 📉 验证节点A 排名 5 → 8（-3）");
        assert!(!message.contains("Node ID"));
        assert!(!message.contains("节点:"));
    }

    #[test]
    fn test_unknown_ranking_does_not_clear_last_known_value() {
        let mut state = NotificationState::default();
        state.plan_ranking_change("node-a", 7);

        assert_eq!(state.plan_ranking_change("node-a", 0), None);

        let change = state
            .plan_ranking_change("node-a", 5)
            .expect("ranking change should still compare against last known rank");

        assert_eq!(
            change,
            RankingChange {
                previous: 7,
                current: 5,
            }
        );
    }

    #[test]
    fn test_ranking_change_uses_fallback_name_without_node_id() {
        let notifier = create_test_notifier();

        let message = notifier.render_ranking_changed_message(
            "",
            RankingChange {
                previous: 7,
                current: 5,
            },
        );

        assert_eq!(message, "[chaindash] 📈 未命名节点 排名 7 → 5（+2）");
        assert!(!message.contains("node-a-id"));
    }

    #[test]
    fn test_rate_limit_suppresses_repeated_delivery_within_window() {
        let mut state = NotificationState::default();
        let now = Instant::now();
        let rate_limit = Duration::from_secs(60);

        assert!(state.allow_delivery("ranking-changed:node-a", now, rate_limit));
        assert!(!state.allow_delivery(
            "ranking-changed:node-a",
            now + Duration::from_secs(59),
            rate_limit,
        ));
        assert!(state.allow_delivery(
            "ranking-changed:node-a",
            now + Duration::from_secs(60),
            rate_limit,
        ));
    }
}
