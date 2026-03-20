//! # channels 表 —— 群组实体
//!
//! ## 业务规则
//! - 创建者自动成为 admin
//! - is_closed = true 时禁止发送新消息，但成员和历史消息保留
//! - created_by 关联 agents 表（逻辑外键，创建者）
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "channels")]
pub struct Model {
    /// 主键，UUID
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 群组名称
    pub name: String,

    /// 创建者 agent ID（逻辑外键 → agents.id）
    pub created_by: String,

    /// 是否已关闭
    #[sea_orm(default_value = false)]
    pub is_closed: bool,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,

    /// 更新时间，UTC
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
