use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "servers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub node_id: Uuid,
    #[sea_orm(nullable)]
    pub allocation_id: Option<Uuid>,
    pub image_id: Uuid,
    pub cpu_limit: i32,
    pub ram_limit: i32,
    pub disk_limit: i32,
    pub swap_limit: i32,
    pub backup_limit: i32,
    pub io_weight: i32,
    pub oom_killer: bool,
    #[sea_orm(column_type = "Text")]
    pub docker_image: String,
    #[sea_orm(column_type = "Text")]
    pub startup_command: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub cpu_pinning: Option<String>,
    pub status: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::nodes::Entity",
        from = "Column::NodeId",
        to = "super::nodes::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Node,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::OwnerId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Owner,
    #[sea_orm(
        belongs_to = "super::images::Entity",
        from = "Column::ImageId",
        to = "super::images::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Image,
    #[sea_orm(
        belongs_to = "super::allocations::Entity",
        from = "Column::AllocationId",
        to = "super::allocations::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    PrimaryAllocation,
}

impl Related<super::nodes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Node.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Owner.def()
    }
}

impl Related<super::images::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Image.def()
    }
}

impl Related<super::allocations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PrimaryAllocation.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
