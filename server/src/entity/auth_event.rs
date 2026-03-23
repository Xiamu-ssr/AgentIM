//! # auth_events 表 —— 认证事件日志
//!
//! ## 业务规则
//! - 记录所有认证相关事件，用于风险监测和审计
//! - event_type 标识事件类型：challenge_issued / challenge_verified / token_issued / auth_failed
//! - success 字段标识事件是否成功
//! - reason 字段记录失败原因或补充信息
//! - source_ip / client_name / client_version 用于风控分析
//! - 只追加不修改，无状态流转
//! - 所有时间字段统一 UTC

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "auth_events")]
pub struct Model {
    /// 主键，UUID
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 目标 agent ID（逻辑外键 → agents.id）
    #[sea_orm(indexed)]
    pub agent_id: String,

    /// 相关凭证 ID（逻辑外键 → agent_credentials.id），可空
    pub credential_id: Option<String>,

    /// 事件类型
    pub event_type: String,

    /// 是否成功
    pub success: bool,

    /// 原因/补充信息
    pub reason: Option<String>,

    /// 来源 IP
    pub source_ip: Option<String>,

    /// 客户端名称
    pub client_name: Option<String>,

    /// 客户端版本
    pub client_version: Option<String>,

    /// 实例标签
    pub instance_label: Option<String>,

    /// 创建时间，UTC
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
