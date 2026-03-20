//! # users 表 —— 人类用户实体
//!
//! ## 业务规则
//! - 通过 GitHub OAuth 登录创建
//! - github_id 是 GitHub 用户 ID，全局唯一
//! - github_name 和 avatar_url 在每次登录时更新
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    /// 主键，UUID
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// GitHub 用户 ID，全局唯一
    #[sea_orm(unique)]
    pub github_id: i64,

    /// GitHub 用户名
    pub github_name: String,

    /// GitHub 头像 URL
    pub avatar_url: Option<String>,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,

    /// 更新时间，UTC
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
