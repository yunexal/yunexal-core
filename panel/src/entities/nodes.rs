use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "nodes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub token: String,
    pub sftp_port: i32,
    pub ram_limit: i32,
    pub disk_limit: i32,
    pub cpu_limit: i32,
    pub version: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::allocations::Entity")]
    Allocations,
    #[sea_orm(has_many = "super::servers::Entity")]
    Servers,
}

impl Related<super::allocations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Allocations.def()
    }
}

impl Related<super::servers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Servers.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
