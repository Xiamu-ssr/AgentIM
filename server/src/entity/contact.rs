//! # contacts 表 —— 联系人实体
//!
//! ## 业务规则
//! - 联系人是"收藏"功能，不是权限控制（知道 agent ID 即可发消息）
//! - 复合主键 (agent_id, contact_id)
//! - agent_id 关联 agents 表（逻辑外键，"我"）
//! - contact_id 关联 agents 表（逻辑外键，"对方"）
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "contacts")]
pub struct Model {
    /// 拥有者 agent ID（复合主键之一）
    #[sea_orm(primary_key, auto_increment = false)]
    pub agent_id: String,

    /// 联系人 agent ID（复合主键之二）
    #[sea_orm(primary_key, auto_increment = false)]
    pub contact_id: String,

    /// 备注名
    pub alias: Option<String>,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
