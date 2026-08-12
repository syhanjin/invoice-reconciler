use sea_orm::entity::prelude::*;

pub mod app_setting {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "app_settings")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: i32,
        pub ollama_base_url: String,
        pub ollama_model: String,
        pub max_concurrency: i32,
        pub updated_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod import_batch {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "import_batches")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub platform: String,
        pub status: String,
        pub created_at: String,
        pub completed_at: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::import_task::Entity")]
        ImportTasks,
    }

    impl Related<super::import_task::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::ImportTasks.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod import_task {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "import_tasks")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub batch_id: String,
        pub original_file_name: String,
        pub managed_image_path: Option<String>,
        pub image_sha256: String,
        pub status: String,
        pub error_code: Option<String>,
        pub error_message: Option<String>,
        pub warning_message: Option<String>,
        pub retry_count: i32,
        pub created_at: String,
        pub started_at: Option<String>,
        pub completed_at: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::import_batch::Entity",
            from = "Column::BatchId",
            to = "super::import_batch::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        ImportBatch,
        #[sea_orm(has_one = "super::order::Entity")]
        Order,
    }

    impl Related<super::import_batch::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::ImportBatch.def()
        }
    }

    impl Related<super::order::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Order.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod order {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "orders")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub platform: String,
        pub external_order_number: Option<String>,
        pub paid_amount_fen: Option<i64>,
        pub ordered_at: Option<String>,
        pub shop_name: Option<String>,
        pub raw_ocr_json: String,
        pub possible_duplicate: bool,
        pub created_at: String,
        pub updated_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::import_task::Entity",
            from = "Column::Id",
            to = "super::import_task::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        ImportTask,
        #[sea_orm(has_many = "super::order_item::Entity")]
        OrderItems,
    }

    impl Related<super::import_task::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::ImportTask.def()
        }
    }

    impl Related<super::order_item::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::OrderItems.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod order_item {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "order_items")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub order_id: String,
        pub position: i32,
        pub product_name: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::order::Entity",
            from = "Column::OrderId",
            to = "super::order::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        Order,
    }

    impl Related<super::order::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Order.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}
