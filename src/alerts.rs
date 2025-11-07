use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use colored::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

impl AlertSeverity {
    pub fn color(&self) -> &str {
        match self {
            AlertSeverity::Critical => "red",
            AlertSeverity::Warning => "yellow",
            AlertSeverity::Info => "blue",
        }
    }

    pub fn emoji(&self) -> &str {
        match self {
            AlertSeverity::Critical => "🚨",
            AlertSeverity::Warning => "⚠️",
            AlertSeverity::Info => "ℹ️",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertStatus {
    Firing,
    Acknowledged,
    Silenced,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: AlertSeverity,
    pub status: AlertStatus,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

impl Alert {
    pub fn new(title: String, description: String, severity: AlertSeverity, source: String) -> Self {
        let now = Utc::now();
        Alert {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            severity,
            status: AlertStatus::Firing,
            source,
            created_at: now,
            updated_at: now,
            acknowledged_at: None,
            resolved_at: None,
            metadata: HashMap::new(),
        }
    }

    pub fn acknowledge(&mut self) {
        self.status = AlertStatus::Acknowledged;
        self.acknowledged_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn resolve(&mut self) {
        self.status = AlertStatus::Resolved;
        self.resolved_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn silence(&mut self) {
        self.status = AlertStatus::Silenced;
        self.updated_at = Utc::now();
    }

    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub enabled: bool,
    pub check_interval_seconds: u64,
    pub disk_usage_threshold: u8,
    pub notification_channels: Vec<NotificationChannel>,
}

impl Default for AlertConfig {
    fn default() -> Self {
        AlertConfig {
            enabled: true,
            check_interval_seconds: 300, // 5 minutes
            disk_usage_threshold: 90,
            notification_channels: vec![NotificationChannel::Console],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationChannel {
    Console,
    Webhook { url: String },
    Slack { webhook_url: String },
    Email { smtp_server: String, from: String, to: Vec<String> },
}

pub struct AlertManager {
    alerts: Vec<Alert>,
    config: AlertConfig,
    storage_path: PathBuf,
}

impl AlertManager {
    pub fn new(storage_path: PathBuf) -> Result<Self> {
        let config = AlertConfig::default();
        let alerts = Self::load_alerts(&storage_path)?;

        Ok(AlertManager {
            alerts,
            config,
            storage_path,
        })
    }

    pub fn with_config(storage_path: PathBuf, config: AlertConfig) -> Result<Self> {
        let alerts = Self::load_alerts(&storage_path)?;

        Ok(AlertManager {
            alerts,
            config,
            storage_path,
        })
    }

    fn load_alerts(path: &Path) -> Result<Vec<Alert>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let contents = fs::read_to_string(path)
            .context("Failed to read alerts storage")?;

        if contents.is_empty() {
            return Ok(Vec::new());
        }

        let alerts: Vec<Alert> = serde_json::from_str(&contents)
            .context("Failed to parse alerts JSON")?;

        Ok(alerts)
    }

    fn save_alerts(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create storage directory")?;
        }

        let json = serde_json::to_string_pretty(&self.alerts)
            .context("Failed to serialize alerts")?;

        fs::write(&self.storage_path, json)
            .context("Failed to write alerts to storage")?;

        Ok(())
    }

    pub fn create_alert(&mut self, alert: Alert) -> Result<String> {
        // Check for duplicate active alerts with same title
        let has_duplicate = self.alerts.iter().any(|a| {
            a.title == alert.title &&
            matches!(a.status, AlertStatus::Firing | AlertStatus::Acknowledged)
        });

        if has_duplicate {
            return Ok("Duplicate alert suppressed".to_string());
        }

        let alert_id = alert.id.clone();

        // Send notifications
        self.notify(&alert)?;

        self.alerts.push(alert);
        self.save_alerts()?;

        Ok(alert_id)
    }

    pub fn acknowledge_alert(&mut self, alert_id: &str) -> Result<()> {
        let alert = self.alerts.iter_mut()
            .find(|a| a.id == alert_id)
            .context("Alert not found")?;

        alert.acknowledge();
        self.save_alerts()?;

        Ok(())
    }

    pub fn resolve_alert(&mut self, alert_id: &str) -> Result<()> {
        let alert = self.alerts.iter_mut()
            .find(|a| a.id == alert_id)
            .context("Alert not found")?;

        alert.resolve();
        self.save_alerts()?;

        Ok(())
    }

    pub fn silence_alert(&mut self, alert_id: &str) -> Result<()> {
        let alert = self.alerts.iter_mut()
            .find(|a| a.id == alert_id)
            .context("Alert not found")?;

        alert.silence();
        self.save_alerts()?;

        Ok(())
    }

    pub fn get_alerts(&self, filter: Option<AlertStatus>) -> Vec<&Alert> {
        match filter {
            Some(status) => self.alerts.iter()
                .filter(|a| a.status == status)
                .collect(),
            None => self.alerts.iter().collect(),
        }
    }

    pub fn get_alert(&self, alert_id: &str) -> Option<&Alert> {
        self.alerts.iter().find(|a| a.id == alert_id)
    }

    fn notify(&self, alert: &Alert) -> Result<()> {
        for channel in &self.config.notification_channels {
            if let Err(e) = self.send_notification(channel, alert) {
                eprintln!("{} Failed to send notification via {:?}: {}",
                    "Warning:".yellow(), channel, e);
            }
        }
        Ok(())
    }

    fn send_notification(&self, channel: &NotificationChannel, alert: &Alert) -> Result<()> {
        match channel {
            NotificationChannel::Console => {
                self.print_alert_notification(alert);
                Ok(())
            }
            NotificationChannel::Webhook { url } => {
                self.send_webhook_notification(url, alert)
            }
            NotificationChannel::Slack { webhook_url } => {
                self.send_slack_notification(webhook_url, alert)
            }
            NotificationChannel::Email { smtp_server, from, to } => {
                // Email sending would require additional dependencies
                // For now, just log it
                println!("{} Email notification would be sent to: {:?}",
                    "Info:".blue(), to);
                println!("  From: {}", from);
                println!("  SMTP: {}", smtp_server);
                Ok(())
            }
        }
    }

    fn print_alert_notification(&self, alert: &Alert) {
        println!("\n{}", "=".repeat(80).bright_black());
        println!("{} {} {}",
            alert.severity.emoji(),
            "NEW ALERT".bold(),
            alert.severity.emoji());
        println!("{}", "=".repeat(80).bright_black());
        println!("{} {}", "Severity:".cyan().bold(),
            format!("{:?}", alert.severity).color(alert.severity.color()).bold());
        println!("{} {}", "Title:".cyan().bold(), alert.title.bright_white());
        println!("{} {}", "Description:".cyan().bold(), alert.description);
        println!("{} {}", "Source:".cyan().bold(), alert.source.bright_yellow());
        println!("{} {}", "Alert ID:".cyan().bold(), alert.id.truecolor(150, 150, 150));
        println!("{} {}", "Created:".cyan().bold(), alert.created_at.format("%Y-%m-%d %H:%M:%S UTC"));

        if !alert.metadata.is_empty() {
            println!("\n{}", "Metadata:".cyan().bold());
            for (key, value) in &alert.metadata {
                println!("  {} {}",
                    format!("{}:", key).truecolor(180, 180, 180),
                    value.bright_white());
            }
        }
        println!("{}", "=".repeat(80).bright_black());
    }

    fn send_webhook_notification(&self, url: &str, alert: &Alert) -> Result<()> {
        let payload = serde_json::json!({
            "alert_id": alert.id,
            "title": alert.title,
            "description": alert.description,
            "severity": alert.severity,
            "status": alert.status,
            "source": alert.source,
            "created_at": alert.created_at,
            "metadata": alert.metadata,
        });

        let client = reqwest::blocking::Client::new();
        let response = client.post(url)
            .json(&payload)
            .send()
            .context("Failed to send webhook")?;

        if !response.status().is_success() {
            anyhow::bail!("Webhook returned error: {}", response.status());
        }

        Ok(())
    }

    fn send_slack_notification(&self, webhook_url: &str, alert: &Alert) -> Result<()> {
        let color = match alert.severity {
            AlertSeverity::Critical => "#FF0000",
            AlertSeverity::Warning => "#FFA500",
            AlertSeverity::Info => "#0000FF",
        };

        let payload = serde_json::json!({
            "attachments": [{
                "color": color,
                "title": format!("{} {}", alert.severity.emoji(), alert.title),
                "text": alert.description,
                "fields": [
                    {
                        "title": "Severity",
                        "value": format!("{:?}", alert.severity),
                        "short": true
                    },
                    {
                        "title": "Source",
                        "value": alert.source,
                        "short": true
                    },
                    {
                        "title": "Alert ID",
                        "value": alert.id,
                        "short": false
                    }
                ],
                "footer": "catdog alerting system",
                "ts": alert.created_at.timestamp()
            }]
        });

        let client = reqwest::blocking::Client::new();
        let response = client.post(webhook_url)
            .json(&payload)
            .send()
            .context("Failed to send Slack notification")?;

        if !response.status().is_success() {
            anyhow::bail!("Slack webhook returned error: {}", response.status());
        }

        Ok(())
    }
}

pub fn display_alerts(alerts: &[&Alert]) {
    if alerts.is_empty() {
        println!("{}", "No alerts found".yellow());
        return;
    }

    println!("{:<38} {:<10} {:<30} {:<15} {:<20}",
             "ID".cyan().bold(),
             "SEVERITY".cyan().bold(),
             "TITLE".cyan().bold(),
             "STATUS".cyan().bold(),
             "CREATED".cyan().bold());
    println!("{}", "=".repeat(120).bright_black());

    for alert in alerts {
        let severity_str = format!("{:?}", alert.severity);
        let status_str = format!("{:?}", alert.status);

        let severity_colored = severity_str.color(alert.severity.color());

        let status_colored = match alert.status {
            AlertStatus::Firing => status_str.red(),
            AlertStatus::Acknowledged => status_str.yellow(),
            AlertStatus::Silenced => status_str.bright_black(),
            AlertStatus::Resolved => status_str.green(),
        };

        println!("{:<38} {:<10} {:<30} {:<15} {}",
                 alert.id.truecolor(150, 150, 150).to_string(),
                 severity_colored.to_string(),
                 alert.title.bright_white().to_string(),
                 status_colored.to_string(),
                 alert.created_at.format("%Y-%m-%d %H:%M:%S"));
    }

    println!("\n{} Total alerts: {}",
        "ℹ️".blue(),
        alerts.len().to_string().bright_white().bold());
}

pub fn display_alert_detail(alert: &Alert) {
    println!("\n{}", "=".repeat(80).bright_black());
    println!("{} {} {}",
        alert.severity.emoji(),
        "ALERT DETAILS".bold(),
        alert.severity.emoji());
    println!("{}", "=".repeat(80).bright_black());

    println!("{} {}", "Alert ID:".cyan().bold(), alert.id.truecolor(150, 150, 150));
    println!("{} {}", "Title:".cyan().bold(), alert.title.bright_white());
    println!("{} {}", "Description:".cyan().bold(), alert.description);
    println!("{} {}", "Severity:".cyan().bold(),
        format!("{:?}", alert.severity).color(alert.severity.color()).bold());

    let status_str = format!("{:?}", alert.status);
    let status_colored = match alert.status {
        AlertStatus::Firing => status_str.red(),
        AlertStatus::Acknowledged => status_str.yellow(),
        AlertStatus::Silenced => status_str.bright_black(),
        AlertStatus::Resolved => status_str.green(),
    };
    println!("{} {}", "Status:".cyan().bold(), status_colored.bold());

    println!("{} {}", "Source:".cyan().bold(), alert.source.bright_yellow());
    println!("{} {}", "Created:".cyan().bold(),
        alert.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("{} {}", "Updated:".cyan().bold(),
        alert.updated_at.format("%Y-%m-%d %H:%M:%S UTC"));

    if let Some(ack_time) = alert.acknowledged_at {
        println!("{} {}", "Acknowledged:".cyan().bold(),
            ack_time.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    if let Some(resolved_time) = alert.resolved_at {
        println!("{} {}", "Resolved:".cyan().bold(),
            resolved_time.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    if !alert.metadata.is_empty() {
        println!("\n{}", "Metadata:".cyan().bold());
        for (key, value) in &alert.metadata {
            println!("  {} {}",
                format!("{}:", key).truecolor(180, 180, 180),
                value.bright_white());
        }
    }

    println!("{}", "=".repeat(80).bright_black());
}
