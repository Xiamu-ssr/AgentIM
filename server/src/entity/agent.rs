//! # agents 表 —— Agent 身份实体
//!
//! ## 业务规则
//! - id 是用户自选的可读 slug（如 "alice-research-bot"），全局唯一
//! - 每个 user 最多创建 MAX_AGENTS_PER_USER 个 agent
//! - token_hash 存储 API token 的 SHA-256 哈希，明文只在创建/重置时展示一次
//! - 状态流转：Active ↔ Suspended（双向）
//! - user_id 关联 users 表（逻辑外键）
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "agents")]
pub struct Model {
    /// 主键，用户自选的可读 ID（如 "alice-research-bot"）
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 所属用户 ID（逻辑外键 → users.id）
    #[sea_orm(indexed)]
    pub user_id: String,

    /// 显示名（如 "Alice 的研究助手"）
    pub name: String,

    /// API token 的 SHA-256 哈希
    #[sea_orm(unique)]
    pub token_hash: String,

    /// 可选头像 URL
    pub avatar_url: Option<String>,

    /// 可选简介
    pub bio: Option<String>,

    /// Agent 状态
    #[sea_orm(default_value = "active")]
    pub status: AgentStatus,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,

    /// 更新时间，UTC
    pub updated_at: DateTimeUtc,
}

/// Agent 状态
///
/// 流转规则：
/// - Active → Suspended（挂起）
/// - Suspended → Active（恢复）
#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum AgentStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "suspended")]
    Suspended,
}

impl AgentStatus {
    /// Check whether transitioning from this status to `next` is allowed.
    #[allow(dead_code)]
    pub fn can_transition_to(&self, next: &AgentStatus) -> bool {
        !matches!((self, next),
            (AgentStatus::Active, AgentStatus::Active)
            | (AgentStatus::Suspended, AgentStatus::Suspended)
        )
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_can_transition_to_suspended() {
        assert!(AgentStatus::Active.can_transition_to(&AgentStatus::Suspended));
    }

    #[test]
    fn suspended_can_transition_to_active() {
        assert!(AgentStatus::Suspended.can_transition_to(&AgentStatus::Active));
    }

    #[test]
    fn cannot_transition_to_same_status() {
        assert!(!AgentStatus::Active.can_transition_to(&AgentStatus::Active));
        assert!(!AgentStatus::Suspended.can_transition_to(&AgentStatus::Suspended));
    }
}
