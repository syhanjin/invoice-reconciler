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

pub mod order_import_pool {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "order_import_pool")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub original_file_name: String,
        pub managed_image_path: String,
        pub image_sha256: String,
        pub status: String,
        pub error_code: Option<String>,
        pub error_message: Option<String>,
        pub retry_count: i32,
        pub created_at: String,
        pub started_at: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod order_import_log {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "order_import_logs")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub task_id: String,
        pub order_id: String,
        pub original_file_name: String,
        pub provider: String,
        pub model: String,
        pub warning_message: Option<String>,
        pub completed_at: String,
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
        pub original_file_name: Option<String>,
        pub managed_image_path: Option<String>,
        pub image_sha256: Option<String>,
        pub warning_message: Option<String>,
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

pub mod invoice_batch {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoice_batches")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub status: String,
        pub forced_type: Option<i32>,
        pub created_at: String,
        pub completed_at: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::invoice_task::Entity")]
        InvoiceTasks,
    }

    impl Related<super::invoice_task::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::InvoiceTasks.def()
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod invoice_task {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoice_tasks")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub batch_id: String,
        pub original_file_name: String,
        pub source_path: Option<String>,
        pub source_sha256: String,
        pub media_type: String,
        pub forced_type: Option<i32>,
        pub status: String,
        pub error_code: Option<String>,
        pub error_message: Option<String>,
        pub warning_message: Option<String>,
        pub retry_count: i32,
        pub request_id: Option<String>,
        pub raw_response_json: Option<String>,
        pub existing_invoice_id: Option<String>,
        pub created_at: String,
        pub started_at: Option<String>,
        pub completed_at: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::invoice_batch::Entity",
            from = "Column::BatchId",
            to = "super::invoice_batch::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        InvoiceBatch,
        #[sea_orm(has_many = "super::invoice::Entity")]
        Invoices,
    }
    impl Related<super::invoice_batch::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::InvoiceBatch.def()
        }
    }
    impl Related<super::invoice::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Invoices.def()
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod invoice_recovery_marker {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoice_recovery_markers")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub source_sha256: String,
        #[sea_orm(primary_key, auto_increment = false)]
        pub recovery_identity: String,
        pub recovery_task_id: Option<String>,
        pub deleted_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::invoice_task::Entity",
            from = "Column::RecoveryTaskId",
            to = "super::invoice_task::Column::Id",
            on_update = "NoAction",
            on_delete = "SetNull"
        )]
        RecoveryTask,
    }

    impl Related<super::invoice_task::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::RecoveryTask.def()
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod invoice {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoices")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub task_id: String,
        pub r#type: i32,
        pub subtype: String,
        pub type_description: String,
        pub subtype_description: String,
        pub invoice_code: Option<String>,
        pub invoice_number: Option<String>,
        pub invoice_date: Option<String>,
        pub buyer_name: Option<String>,
        pub buyer_tax_id: Option<String>,
        pub seller_name: Option<String>,
        pub seller_tax_id: Option<String>,
        pub amount_fen: Option<i64>,
        pub tax_fen: Option<i64>,
        pub total_fen: Option<i64>,
        pub currency: String,
        pub reliable_identity: Option<String>,
        pub possible_duplicate: bool,
        pub preview_path: String,
        pub page_start: i32,
        pub page_end: i32,
        pub raw_invoice_json: String,
        pub created_at: String,
        pub updated_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::invoice_task::Entity",
            from = "Column::TaskId",
            to = "super::invoice_task::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        InvoiceTask,
        #[sea_orm(has_many = "super::invoice_line_item::Entity")]
        Items,
    }
    impl Related<super::invoice_task::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::InvoiceTask.def()
        }
    }
    impl Related<super::invoice_line_item::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Items.def()
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod invoice_line_item {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoice_items")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub invoice_id: String,
        pub position: i32,
        pub name: String,
        pub specification: Option<String>,
        pub unit: Option<String>,
        pub quantity: Option<String>,
        pub unit_price_fen: Option<i64>,
        pub amount_fen: Option<i64>,
        pub tax_rate: Option<String>,
        pub tax_fen: Option<i64>,
        pub raw_json: String,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::invoice::Entity",
            from = "Column::InvoiceId",
            to = "super::invoice::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        Invoice,
    }
    impl Related<super::invoice::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Invoice.def()
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod invoice_source_link {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoice_source_links")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub invoice_id: String,
        pub task_id: String,
        pub page_start: i32,
        pub page_end: i32,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod invoice_match {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoice_matches")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub invoice_id: String,
        pub source: String,
        pub confirmed_at: String,
        pub display_name: Option<String>,
        pub summary_status: String,
        pub summary_error: Option<String>,
        pub summary_provider: Option<String>,
        pub summary_model: Option<String>,
        pub summary_input_hash: Option<String>,
        pub summary_generated_at: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod invoice_match_order {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "invoice_match_orders")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub invoice_id: String,
        #[sea_orm(primary_key, auto_increment = false)]
        pub order_id: String,
        pub position: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod project {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "projects")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        pub created_at: String,
        pub updated_at: String,
        pub archived_at: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::project_invoice::Entity")]
        ProjectInvoices,
    }

    impl Related<super::project_invoice::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::ProjectInvoices.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod project_invoice {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "project_invoices")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub project_id: String,
        #[sea_orm(primary_key, auto_increment = false)]
        pub invoice_id: String,
        pub position: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::project::Entity",
            from = "Column::ProjectId",
            to = "super::project::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        Project,
        #[sea_orm(
            belongs_to = "super::invoice::Entity",
            from = "Column::InvoiceId",
            to = "super::invoice::Column::Id",
            on_update = "NoAction",
            on_delete = "Cascade"
        )]
        Invoice,
    }

    impl Related<super::project::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Project.def()
        }
    }

    impl Related<super::invoice::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Invoice.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}
