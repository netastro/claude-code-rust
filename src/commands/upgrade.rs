//! 升级命令模块
//!
//! 实现Claude Code的升级功能，包括GitHub版本检查和更新下载

use crate::error::Result;
use crate::commands::types::{Command, CommandBase, CommandContext, CommandResult, LocalCommand, LoadedFrom};
use crate::commands::registry::CommandExecutor as CmdExecutor;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::env;

/// 升级命令
pub struct UpgradeCommand;

#[async_trait]
impl CmdExecutor for UpgradeCommand {
    async fn execute(&self, _context: CommandContext) -> Result<CommandResult> {
        match check_for_updates().await {
            Ok(message) => Ok(CommandResult {
                content: message,
                ..Default::default()
            }),
            Err(e) => Ok(CommandResult {
                content: format!("Failed to check for updates: {}", e),
                ..Default::default()
            }),
        }
    }

    fn command(&self) -> Command {
        Command::Local(LocalCommand {
            base: CommandBase {
                name: "upgrade".to_string(),
                description: "Check for updates and upgrade Claude Code".to_string(),
                has_user_specified_description: None,
                aliases: Some(vec!["update".to_string(), "up".to_string()]),
                availability: None,
                is_hidden: None,
                is_mcp: None,
                argument_hint: None,
                when_to_use: None,
                version: None,
                disable_model_invocation: None,
                user_invocable: None,
                loaded_from: Some(LoadedFrom::Bundled),
                kind: None,
                immediate: Some(true),
                is_sensitive: None,
            },
            supports_non_interactive: true,
        })
    }
}

/// 检查更新
async fn check_for_updates() -> Result<String> {
    let current_version = env!("CARGO_PKG_VERSION");

    // 尝试从GitHub API获取最新版本
    let latest_version = match fetch_latest_version_from_github().await {
        Ok(version) => version,
        Err(e) => {
            return Ok(format!(
                "Failed to fetch latest version from GitHub: {}\n\
                 Current version: {}\n\
                 Please check https://github.com/anthropics/claude-code for updates.",
                e, current_version
            ));
        }
    };

    // 简单的版本比较（语义化版本比较需要更复杂的逻辑）
    if latest_version == current_version {
        return Ok(format!(
            "Claude Code Rust is up to date! (v{})",
            current_version
        ));
    }

    // 有新版本可用
    Ok(format!(
        "New version available: v{} (current: v{})\n\
         \n\
         To upgrade:\n\
         1. Visit https://github.com/anthropics/claude-code/releases\n\
         2. Download the latest release\n\
         3. Follow installation instructions\n\
         \n\
         Or build from source:\n\
         git pull origin main\n\
         cargo build --release",
        latest_version, current_version
    ))
}

/// 从GitHub API获取最新版本
async fn fetch_latest_version_from_github() -> Result<String> {
    let client = Client::new();
    let url = "https://api.github.com/repos/anthropics/claude-code/releases/latest";

    let response = client
        .get(url)
        .header("User-Agent", "Claude-Code-Upgrade-Check")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("GitHub API returned status: {}", response.status()).into());
    }

    let json: Value = response.json().await?;

    // 提取tag_name字段（版本号）
    let tag_name = json.get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or("No tag_name in GitHub API response")?;

    // 移除可能的前缀"v"
    let version = if tag_name.starts_with('v') {
        &tag_name[1..]
    } else {
        tag_name
    };

    Ok(version.to_string())
}

/// 运行升级命令（CLI入口点）
pub async fn run() -> Result<()> {
    let message = check_for_updates().await?;
    println!("{}", message);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upgrade_command_creation() {
        let cmd = UpgradeCommand;
        let command = cmd.command();

        assert_eq!(command.name(), "upgrade");
        assert_eq!(command.description(), "Check for updates and upgrade Claude Code");
    }
}