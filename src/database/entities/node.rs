use sea_orm::entity::prelude::*;
use sea_orm::Set;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "nodes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub private_key: String,
    pub listen: String, // JSON array stored as string
    pub addresses: String, // JSON array stored as string
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: Set(uuid::Uuid::new_v4().to_string()),
            created_at: Set(chrono::Utc::now()),
            updated_at: Set(chrono::Utc::now()),
            ..ActiveModelTrait::default()
        }
    }

    fn before_save<'life0, 'async_trait, C>(
        mut self,
        _db: &'life0 C,
        _insert: bool,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Self, DbErr>> + core::marker::Send + 'async_trait>>
    where
        'life0: 'async_trait,
        C: 'async_trait + ConnectionTrait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.updated_at = Set(chrono::Utc::now());
            Ok(self)
        })
    }
}

// Conversion functions between database model and domain model
impl From<Model> for crate::yggdrasil::Node {
    fn from(model: Model) -> Self {
        let listen: Vec<String> = serde_json::from_str(&model.listen).unwrap_or_default();
        let addresses: Vec<String> = serde_json::from_str(&model.addresses).unwrap_or_default();
        
        crate::yggdrasil::Node {
            id: model.id,
            name: model.name,
            public_key: model.public_key,
            private_key: model.private_key,
            listen,
            addresses,
        }
    }
}

impl From<&crate::yggdrasil::Node> for ActiveModel {
    fn from(node: &crate::yggdrasil::Node) -> Self {
        let listen = serde_json::to_string(&node.listen).unwrap_or_default();
        let addresses = serde_json::to_string(&node.addresses).unwrap_or_default();
        
        ActiveModel {
            id: Set(node.id.clone()),
            name: Set(node.name.clone()),
            public_key: Set(node.public_key.clone()),
            private_key: Set(node.private_key.clone()),
            listen: Set(listen),
            addresses: Set(addresses),
            created_at: Set(chrono::Utc::now()),
            updated_at: Set(chrono::Utc::now()),
        }
    }
}