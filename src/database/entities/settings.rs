use sea_orm::entity::prelude::*;
use sea_orm::Set;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: String,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn parse_json_value<T>(&self) -> Result<T, serde_json::Error> 
    where 
        T: for<'de> serde::Deserialize<'de>
    {
        serde_json::from_str(&self.value)
    }
}

impl ActiveModel {
    pub fn new(key: String, value: &impl serde::Serialize) -> Result<Self, serde_json::Error> {
        let value_json = serde_json::to_string(value)?;
        let now = chrono::Utc::now().naive_utc();
        
        Ok(Self {
            key: Set(key),
            value: Set(value_json),
            created_at: Set(now),
            updated_at: Set(now),
        })
    }
    
    pub fn update_value(&mut self, value: &impl serde::Serialize) -> Result<(), serde_json::Error> {
        let value_json = serde_json::to_string(value)?;
        self.value = Set(value_json);
        self.updated_at = Set(chrono::Utc::now().naive_utc());
        Ok(())
    }
}