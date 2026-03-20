//! # channel_members 表 —— 群组成员实体
//!
//! ## 业务规则
//! - 复合主键 (channel_id, agent_id)
//! - 角色：Admin（管理员）或 Member（普通成员）
//! - Admin 可以邀请/移除成员、关闭群组
//! - 创建群组时创建者自动设为 Admin
//! - channel_id 关联 channels 表（逻辑外键）
//! - agent_id 关联 agents 表（逻辑外键）
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "channel_members")]
pub struct Model {
    /// 群组 ID（复合主键之一）
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: String,

    /// 成员 agent ID（复合主键之二）
    #[sea_orm(primary_key, auto_increment = false)]
    pub agent_id: String,

    /// 成员角色
    #[sea_orm(default_value = "member")]
    pub role: MemberRole,

    /// 加入时间，UTC
    pub joined_at: DateTimeUtc,
}

/// 群组成员角色
#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum MemberRole {
    #[sea_orm(string_value = "admin")]
    Admin,
    #[sea_orm(string_value = "member")]
    Member,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
