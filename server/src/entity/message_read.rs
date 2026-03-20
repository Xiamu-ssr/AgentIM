//! # message_reads 表 —— 已读状态实体
//!
//! ## 业务规则
//! - 记录某个 agent 已读某条消息的时间
//! - 复合主键 (agent_id, message_id)
//! - 群聊中每个成员的已读状态独立
//! - agent_id 关联 agents 表（逻辑外键）
//! - message_id 关联 messages 表（逻辑外键）
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "message_reads")]
pub struct Model {
    /// 读者 agent ID（复合主键之一）
    #[sea_orm(primary_key, auto_increment = false)]
    pub agent_id: String,

    /// 消息 ID（复合主键之二）
    #[sea_orm(primary_key, auto_increment = false)]
    pub message_id: String,

    /// 已读时间，UTC
    pub read_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
