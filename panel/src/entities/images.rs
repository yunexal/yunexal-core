use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "images")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub runtime_id: Uuid,
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub docker_images: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub stop_command: String,
    #[sea_orm(column_type = "Text")]
    pub startup_command: String,
    #[sea_orm(column_type = "Text")]
    pub log_config: String,
    #[sea_orm(column_type = "Text")]
    pub config_files: String,
    #[sea_orm(column_type = "Text")]
    pub start_config: String,
    pub requires_port: bool,
    #[sea_orm(column_type = "Text")]
    pub install_script: String,
    #[sea_orm(column_type = "Text")]
    pub install_container: String,
    #[sea_orm(column_type = "Text")]
    pub install_entrypoint: String,
    #[sea_orm(column_type = "Text")]
    pub variables: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::runtimes::Entity",
        from = "Column::RuntimeId",
        to = "super::runtimes::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Runtime,
    #[sea_orm(has_many = "super::servers::Entity")]
    Servers,
}

impl Related<super::runtimes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Runtime.def()
    }
}

impl Related<super::servers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Servers.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
