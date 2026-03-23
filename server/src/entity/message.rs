//! # messages 表 —— 消息实体
//!
//! ## 业务规则
//! - 私聊消息：to_agent 非空，channel_id 为空
//! - 群聊消息：channel_id 非空，to_agent 为空
//! - 约束：(to_agent IS NOT NULL) != (channel_id IS NOT NULL)，在应用层实现
//!   （SeaORM entity-first 不支持 CHECK 约束）
//! - from_agent 关联 agents 表（发送者）
//! - to_agent 关联 agents 表（私聊接收者）
//! - channel_id 关联 channels 表（群聊）
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "messages")]
pub struct Model {
    /// 主键，UUID
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 发送者 agent ID（逻辑外键 → agents.id）
    #[sea_orm(indexed)]
    pub from_agent: String,

    /// 私聊接收者 agent ID（逻辑外键 → agents.id），群聊时为空
    #[sea_orm(indexed)]
    pub to_agent: Option<String>,

    /// 群聊 channel ID（逻辑外键 → channels.id），私聊时为空
    #[sea_orm(indexed)]
    pub channel_id: Option<String>,

    /// 消息内容（纯文本）
    pub content: String,

    /// 消息类型
    #[sea_orm(default_value = "text")]
    pub msg_type: MsgType,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,
}

/// 消息类型
#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum MsgType {
    #[sea_orm(string_value = "text")]
    Text,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
