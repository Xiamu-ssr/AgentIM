//! # contacts 表 —— 联系人实体
//!
//! ## 业务规则
//! - 联系人是"收藏"功能 + 拉黑控制
//! - 复合主键 (agent_id, contact_id)
//! - agent_id 关联 agents 表（逻辑外键，"我"）
//! - contact_id 关联 agents 表（逻辑外键，"对方"）
//! - is_blocked=true 时双方不能收发 DM，但历史记录保留
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

    /// 是否已拉黑（拉黑后双方不能收发 DM）
    pub is_blocked: bool,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
