//! # claim_tokens 表 —— 一次性授权码
//!
//! ## 业务规则
//! - 用于首绑（enroll）和恢复（recover）流程
//! - owner 通过 Web 申请 claim code，服务端返回明文（仅此一次）
//! - 数据库只存 code_hash（SHA-256）
//! - 同一 agent 同时只能有一个 active claim code，旧的自动作废
//! - 有效期 10 分钟（CLAIM_CODE_TTL_SECS）
//! - 使用后标记为 Used，不可复用
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "claim_tokens")]
pub struct Model {
    /// 主键，UUID
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 目标 agent ID（逻辑外键 → agents.id）
    #[sea_orm(indexed)]
    pub agent_id: String,

    /// 申请者 user ID（逻辑外键 → users.id）
    pub user_id: String,

    /// claim code 的 SHA-256 哈希
    pub code_hash: String,

    /// 用途
    pub purpose: ClaimPurpose,

    /// 状态
    #[sea_orm(default_value = "active")]
    pub status: ClaimStatus,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,

    /// 过期时间，UTC
    pub expires_at: DateTimeUtc,

    /// 使用时间，UTC
    pub used_at: Option<DateTimeUtc>,

    /// 申请者 IP 地址
    pub created_from_ip: Option<String>,
}

/// Claim 用途
#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum ClaimPurpose {
    #[sea_orm(string_value = "enroll")]
    Enroll,
    #[sea_orm(string_value = "recover")]
    Recover,
}

/// Claim 状态
///
/// 流转规则：
/// - Active → Used（成功使用）
/// - Active → Revoked（被新 claim code 替代或手动撤销）
/// - Active → Expired（过期，应用层判定）
/// - Used / Revoked / Expired 为终态
#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum ClaimStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "used")]
    Used,
    #[sea_orm(string_value = "revoked")]
    Revoked,
    #[sea_orm(string_value = "expired")]
    Expired,
}

impl ClaimStatus {
    /// Check whether transitioning from this status to `next` is allowed.
    #[allow(dead_code)]
    pub fn can_transition_to(&self, next: &ClaimStatus) -> bool {
        matches!(
            (self, next),
            (ClaimStatus::Active, ClaimStatus::Used)
                | (ClaimStatus::Active, ClaimStatus::Revoked)
                | (ClaimStatus::Active, ClaimStatus::Expired)
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
    fn active_can_transition_to_used() {
        assert!(ClaimStatus::Active.can_transition_to(&ClaimStatus::Used));
    }

    #[test]
    fn active_can_transition_to_revoked() {
        assert!(ClaimStatus::Active.can_transition_to(&ClaimStatus::Revoked));
    }

    #[test]
    fn active_can_transition_to_expired() {
        assert!(ClaimStatus::Active.can_transition_to(&ClaimStatus::Expired));
    }

    #[test]
    fn terminal_states_cannot_transition() {
        for terminal in [ClaimStatus::Used, ClaimStatus::Revoked, ClaimStatus::Expired] {
            for target in [
                ClaimStatus::Active,
                ClaimStatus::Used,
                ClaimStatus::Revoked,
                ClaimStatus::Expired,
            ] {
                assert!(
                    !terminal.can_transition_to(&target),
                    "{:?} should not transition to {:?}",
                    terminal,
                    target
                );
            }
        }
    }
}
