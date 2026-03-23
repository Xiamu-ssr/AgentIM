//! # agent_credentials 表 —— Agent 认证凭证
//!
//! ## 业务规则
//! - 每个 agent 同一时刻只能有一个 status = Active 的凭证
//! - 公钥为 Ed25519，base64 编码存储
//! - public_key_fp 是公钥指纹（SHA-256 前 16 hex），用于快速匹配
//! - 新凭证激活时旧凭证自动变为 Replaced
//! - 风险检测触发时凭证变为 RiskRevoked
//! - 人类 owner 恢复时旧凭证变为 Revoked
//! - replaced_by_id 指向替换此凭证的新凭证 ID
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "agent_credentials")]
pub struct Model {
    /// 主键，UUID
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 所属 agent ID（逻辑外键 → agents.id）
    #[sea_orm(indexed)]
    pub agent_id: String,

    /// Ed25519 公钥（base64 编码）
    pub public_key: String,

    /// 公钥指纹（SHA-256 前 16 hex）
    #[sea_orm(indexed)]
    pub public_key_fp: String,

    /// 凭证状态
    #[sea_orm(default_value = "active")]
    pub status: CredentialStatus,

    /// 吊销原因
    pub revoke_reason: Option<String>,

    /// 用户可见标签（如 "my-laptop"）
    pub instance_label: Option<String>,

    /// 签发时间，UTC
    pub issued_at: DateTimeUtc,

    /// 最后使用时间，UTC
    pub last_used_at: Option<DateTimeUtc>,

    /// 吊销时间，UTC
    pub revoked_at: Option<DateTimeUtc>,

    /// 替换此凭证的新凭证 ID
    pub replaced_by_id: Option<String>,
}

/// 凭证状态
///
/// 流转规则：
/// - Active → Replaced（被新凭证替换）
/// - Active → Revoked（人类 owner 主动吊销/恢复）
/// - Active → RiskRevoked（风险监测自动吊销）
/// - Replaced / Revoked / RiskRevoked 为终态，不可再流转
#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum CredentialStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "revoked")]
    Revoked,
    #[sea_orm(string_value = "risk_revoked")]
    RiskRevoked,
    #[sea_orm(string_value = "replaced")]
    Replaced,
}

impl CredentialStatus {
    /// Check whether transitioning from this status to `next` is allowed.
    #[allow(dead_code)]
    pub fn can_transition_to(&self, next: &CredentialStatus) -> bool {
        matches!(
            (self, next),
            (CredentialStatus::Active, CredentialStatus::Replaced)
                | (CredentialStatus::Active, CredentialStatus::Revoked)
                | (CredentialStatus::Active, CredentialStatus::RiskRevoked)
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
    fn active_can_transition_to_replaced() {
        assert!(CredentialStatus::Active.can_transition_to(&CredentialStatus::Replaced));
    }

    #[test]
    fn active_can_transition_to_revoked() {
        assert!(CredentialStatus::Active.can_transition_to(&CredentialStatus::Revoked));
    }

    #[test]
    fn active_can_transition_to_risk_revoked() {
        assert!(CredentialStatus::Active.can_transition_to(&CredentialStatus::RiskRevoked));
    }

    #[test]
    fn terminal_states_cannot_transition() {
        for terminal in [
            CredentialStatus::Replaced,
            CredentialStatus::Revoked,
            CredentialStatus::RiskRevoked,
        ] {
            for target in [
                CredentialStatus::Active,
                CredentialStatus::Replaced,
                CredentialStatus::Revoked,
                CredentialStatus::RiskRevoked,
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

    #[test]
    fn active_cannot_transition_to_active() {
        assert!(!CredentialStatus::Active.can_transition_to(&CredentialStatus::Active));
    }
}
