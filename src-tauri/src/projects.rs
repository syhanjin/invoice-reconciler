use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, Statement,
    TransactionTrait,
};
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::db::{Database, now};
use crate::entities::{invoice, project, project_invoice};
use crate::error::{AppError, AppResult, CommandResult};
use crate::files::resolve_managed_path;
use crate::invoice_files::resolve_invoice_path;
use crate::models::{
    AssignInvoicesResult, AssignInvoicesToProjectRequest, CreateProjectAndAssignInvoicesRequest,
    IdRequest, InvoiceIdsRequest, InvoiceProjectConflict, ProjectDetail, ProjectInvoiceMember,
    ProjectNameRequest, ProjectOrderSummary, ProjectSummary, ProjectsPage, ProjectsQuery,
    RenameProjectRequest, ReorderProjectInvoicesRequest,
};

#[derive(Clone, Debug, FromQueryResult)]
struct ProjectMemberRow {
    id: String,
    position: i32,
    type_description: String,
    invoice_code: Option<String>,
    invoice_number: Option<String>,
    invoice_date: Option<String>,
    buyer_name: Option<String>,
    buyer_tax_id: Option<String>,
    seller_name: Option<String>,
    seller_tax_id: Option<String>,
    amount_fen: Option<i64>,
    tax_fen: Option<i64>,
    total_fen: Option<i64>,
    currency: String,
    preview_path: String,
    page_start: i32,
    page_end: i32,
    raw_invoice_json: String,
    source_path: Option<String>,
    media_type: String,
    raw_response_json: Option<String>,
    confirmed_source: Option<String>,
    generated_display_name: Option<String>,
    summary_status: Option<String>,
    summary_source: Option<String>,
    summary_error: Option<String>,
    summary_provider: Option<String>,
    summary_model: Option<String>,
    summary_generated_at: Option<String>,
}

#[derive(Clone, Debug, FromQueryResult)]
struct ProjectOrderRow {
    id: String,
    external_order_number: Option<String>,
    paid_amount_fen: Option<i64>,
    managed_image_path: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectExportOrder {
    pub id: String,
    pub external_order_number: Option<String>,
    pub image_path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectExportInvoice {
    pub member: ProjectInvoiceMember,
    pub material_path: PathBuf,
    pub orders: Vec<ProjectExportOrder>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectExportSnapshot {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub invoices: Vec<ProjectExportInvoice>,
}

impl ProjectExportSnapshot {
    fn into_detail(self) -> ProjectDetail {
        let invoices = self
            .invoices
            .into_iter()
            .map(|invoice| invoice.member)
            .collect::<Vec<_>>();
        detail_from_members(
            self.id,
            self.name,
            self.created_at,
            self.updated_at,
            self.archived_at,
            invoices,
        )
    }
}

impl Database {
    pub async fn projects_page(
        &self,
        query: ProjectsQuery,
        app_data: &Path,
    ) -> AppResult<ProjectsPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let search = query
            .search
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let mut finder = project::Entity::find().filter(if query.archived.unwrap_or(false) {
            project::Column::ArchivedAt.is_not_null()
        } else {
            project::Column::ArchivedAt.is_null()
        });
        if let Some(search) = &search {
            finder = finder.filter(project::Column::Name.contains(search));
        }
        let total = finder.clone().count(&self.connection).await? as i64;
        let projects = finder
            .order_by_desc(project::Column::UpdatedAt)
            .order_by_desc(project::Column::Id)
            .offset(u64::from((page - 1) * page_size))
            .limit(u64::from(page_size))
            .all(&self.connection)
            .await?;
        let mut items = Vec::with_capacity(projects.len());
        for value in projects {
            let detail = self.project_snapshot(value, app_data).await?.into_detail();
            items.push(summary_from_detail(detail));
        }
        Ok(ProjectsPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub async fn project_detail(&self, id: &str, app_data: &Path) -> AppResult<ProjectDetail> {
        let value = project::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("项目 {id}")))?;
        Ok(self.project_snapshot(value, app_data).await?.into_detail())
    }

    pub(crate) async fn project_export_snapshot(
        &self,
        id: &str,
        app_data: &Path,
    ) -> AppResult<ProjectExportSnapshot> {
        let value = project::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("项目 {id}")))?;
        self.project_snapshot(value, app_data).await
    }

    async fn project_snapshot(
        &self,
        project_value: project::Model,
        app_data: &Path,
    ) -> AppResult<ProjectExportSnapshot> {
        let rows = ProjectMemberRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT i.id,pi.position,i.type_description,i.invoice_code,i.invoice_number,i.invoice_date,\
             i.buyer_name,i.buyer_tax_id,i.seller_name,i.seller_tax_id,i.amount_fen,i.tax_fen,\
             i.total_fen,i.currency,i.preview_path,i.page_start,i.page_end,i.raw_invoice_json,\
             t.source_path,t.media_type,t.raw_response_json,m.source confirmed_source,\
             m.display_name generated_display_name,m.summary_status,m.summary_source,m.summary_error,\
             m.summary_provider,m.summary_model,m.summary_generated_at \
             FROM project_invoices pi JOIN invoices i ON i.id=pi.invoice_id \
             JOIN invoice_tasks t ON t.id=i.task_id \
             LEFT JOIN invoice_matches m ON m.invoice_id=i.id \
             WHERE pi.project_id=? ORDER BY pi.position,i.id",
            [project_value.id.clone().into()],
        ))
        .all(&self.connection)
        .await?;

        let mut invoices = Vec::with_capacity(rows.len());
        for row in rows {
            let order_rows = ProjectOrderRow::find_by_statement(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT o.id,o.external_order_number,o.paid_amount_fen,o.managed_image_path \
                 FROM invoice_match_orders mo JOIN orders o ON o.id=mo.order_id \
                 WHERE mo.invoice_id=? ORDER BY mo.position,o.id",
                [row.id.clone().into()],
            ))
            .all(&self.connection)
            .await?;
            let mut public_orders = Vec::with_capacity(order_rows.len());
            let mut export_orders = Vec::with_capacity(order_rows.len());
            let mut order_files_available = true;
            for order_row in order_rows {
                let product_names = order_product_names(&self.connection, &order_row.id).await?;
                let image_path = order_row
                    .managed_image_path
                    .as_deref()
                    .and_then(|value| resolve_managed_path(app_data, value).ok());
                let image_available = image_path.as_ref().is_some_and(|path| path.is_file());
                order_files_available &= image_available;
                public_orders.push(ProjectOrderSummary {
                    id: order_row.id.clone(),
                    external_order_number: order_row.external_order_number.clone(),
                    paid_amount_fen: order_row.paid_amount_fen,
                    product_names: product_names.clone(),
                });
                if order_row.paid_amount_fen.is_some()
                    && let Some(image_path) = image_path
                {
                    export_orders.push(ProjectExportOrder {
                        id: order_row.id,
                        external_order_number: order_row.external_order_number,
                        image_path,
                    });
                }
            }
            let preview_path = resolve_invoice_path(app_data, &row.preview_path).ok();
            let material_path = preview_path
                .as_ref()
                .and_then(|_| preferred_invoice_source(app_data, &row))
                .or_else(|| preview_path.clone());
            let material_available = material_path.as_ref().is_some_and(|path| path.is_file());
            let confirmed = row.confirmed_source.is_some();
            let order_amount_fen = confirmed
                .then(|| {
                    public_orders
                        .iter()
                        .map(|order| order.paid_amount_fen)
                        .collect::<Option<Vec<_>>>()
                        .and_then(|amounts| {
                            amounts
                                .into_iter()
                                .try_fold(0_i64, |total, amount| total.checked_add(amount))
                        })
                })
                .flatten();
            let invoice_amount_fen = row.total_fen;
            let actual_amount_fen = actual_amount(invoice_amount_fen, order_amount_fen);
            let blocking_reason = member_blocking_reason(
                confirmed,
                &row.currency,
                invoice_amount_fen,
                order_amount_fen,
                &public_orders,
                material_available,
                order_files_available,
            );
            let file_available = material_available && order_files_available;
            let product_names = public_orders
                .iter()
                .flat_map(|order| order.product_names.iter().cloned())
                .collect::<Vec<_>>();
            let order_labels = public_orders
                .iter()
                .map(|order| {
                    order
                        .external_order_number
                        .clone()
                        .unwrap_or_else(|| order.id.clone())
                })
                .collect::<Vec<_>>();
            let display_name = crate::match_summary::display_name(
                row.generated_display_name.as_deref(),
                &product_names,
                &order_labels,
            );
            let member = ProjectInvoiceMember {
                id: row.id,
                display_name,
                summary_status: row.summary_status,
                summary_source: row.summary_source,
                summary_error: row.summary_error,
                summary_provider: row.summary_provider,
                summary_model: row.summary_model,
                summary_generated_at: row.summary_generated_at,
                position: row.position,
                type_description: row.type_description,
                invoice_code: row.invoice_code,
                invoice_number: row.invoice_number,
                invoice_date: row.invoice_date,
                buyer_name: row.buyer_name,
                buyer_tax_id: row.buyer_tax_id,
                seller_name: row.seller_name,
                seller_tax_id: row.seller_tax_id,
                amount_fen: row.amount_fen,
                tax_fen: row.tax_fen,
                invoice_amount_fen,
                order_amount_fen,
                actual_amount_fen,
                currency: row.currency,
                confirmed,
                file_available,
                blocking_reason,
                orders: public_orders,
            };
            invoices.push(ProjectExportInvoice {
                member,
                material_path: material_path.unwrap_or_default(),
                orders: export_orders,
            });
        }
        Ok(ProjectExportSnapshot {
            id: project_value.id,
            name: project_value.name,
            created_at: project_value.created_at,
            updated_at: project_value.updated_at,
            archived_at: project_value.archived_at,
            invoices,
        })
    }

    pub async fn create_project(&self, name: String, app_data: &Path) -> AppResult<ProjectDetail> {
        let name = validate_project_name(&name)?;
        if project::Entity::find()
            .filter(project::Column::Name.eq(&name))
            .one(&self.connection)
            .await?
            .is_some()
        {
            return Err(AppError::InvalidInput("项目名称已存在".into()));
        }
        let id = Uuid::now_v7().to_string();
        let timestamp = now();
        project::ActiveModel {
            id: Set(id.clone()),
            name: Set(name),
            created_at: Set(timestamp.clone()),
            updated_at: Set(timestamp),
            archived_at: Set(None),
        }
        .insert(&self.connection)
        .await
        .map_err(map_project_write_error)?;
        self.project_detail(&id, app_data).await
    }

    pub async fn rename_project(
        &self,
        id: &str,
        name: String,
        app_data: &Path,
    ) -> AppResult<ProjectDetail> {
        let name = validate_project_name(&name)?;
        if project::Entity::find()
            .filter(project::Column::Name.eq(&name))
            .filter(project::Column::Id.ne(id))
            .one(&self.connection)
            .await?
            .is_some()
        {
            return Err(AppError::InvalidInput("项目名称已存在".into()));
        }
        let value = project::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("项目 {id}")))?;
        if value.archived_at.is_some() {
            return Err(AppError::InvalidInput("归档项目不能重命名".into()));
        }
        let mut active: project::ActiveModel = value.into();
        active.name = Set(name);
        active.updated_at = Set(now());
        active
            .update(&self.connection)
            .await
            .map_err(map_project_write_error)?;
        self.project_detail(id, app_data).await
    }

    pub async fn delete_project(&self, id: &str) -> AppResult<()> {
        let result = project::Entity::delete_by_id(id)
            .exec(&self.connection)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("项目 {id}")));
        }
        Ok(())
    }

    pub async fn set_project_archived(&self, id: &str, archived: bool) -> AppResult<()> {
        let value = project::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("项目 {id}")))?;
        if value.archived_at.is_some() == archived {
            return Ok(());
        }
        let mut active: project::ActiveModel = value.into();
        active.archived_at = Set(archived.then(now));
        active.updated_at = Set(now());
        active.update(&self.connection).await?;
        Ok(())
    }

    pub async fn create_project_and_assign_invoices(
        &self,
        request: CreateProjectAndAssignInvoicesRequest,
        app_data: &Path,
    ) -> AppResult<ProjectDetail> {
        let name = validate_project_name(&request.name)?;
        let invoice_ids = validate_invoice_ids(request.invoice_ids)?;
        let transaction = self.connection.begin().await?;
        if project::Entity::find()
            .filter(project::Column::Name.eq(&name))
            .one(&transaction)
            .await?
            .is_some()
        {
            return Err(AppError::InvalidInput("项目名称已存在".into()));
        }
        ensure_invoices_exist(&transaction, &invoice_ids).await?;
        if project_invoice::Entity::find()
            .filter(project_invoice::Column::InvoiceId.is_in(invoice_ids.clone()))
            .count(&transaction)
            .await?
            > 0
        {
            return Err(AppError::InvalidInput(
                "所选发票已有项目归属，请刷新后重试".into(),
            ));
        }
        let id = Uuid::now_v7().to_string();
        let timestamp = now();
        project::ActiveModel {
            id: Set(id.clone()),
            name: Set(name),
            created_at: Set(timestamp.clone()),
            updated_at: Set(timestamp),
            archived_at: Set(None),
        }
        .insert(&transaction)
        .await
        .map_err(map_project_write_error)?;
        for (position, invoice_id) in invoice_ids.into_iter().enumerate() {
            project_invoice::ActiveModel {
                project_id: Set(id.clone()),
                invoice_id: Set(invoice_id),
                position: Set(i32::try_from(position).unwrap_or(i32::MAX)),
            }
            .insert(&transaction)
            .await?;
        }
        transaction.commit().await?;
        self.project_detail(&id, app_data).await
    }

    pub async fn assign_invoices_to_project(
        &self,
        request: AssignInvoicesToProjectRequest,
    ) -> AppResult<AssignInvoicesResult> {
        let invoice_ids = validate_invoice_ids(request.invoice_ids)?;
        let transaction = self.connection.begin().await?;
        ensure_project_active(&transaction, &request.project_id).await?;
        ensure_invoices_exist(&transaction, &invoice_ids).await?;
        let conflicts =
            current_project_conflicts(&transaction, &invoice_ids, &request.project_id).await?;
        if !conflicts.is_empty() && !request.accept_reassignment {
            transaction.rollback().await?;
            return Ok(AssignInvoicesResult {
                assigned: false,
                conflicts,
            });
        }

        let source_projects = conflicts
            .iter()
            .map(|conflict| conflict.source_project_id.clone())
            .collect::<HashSet<_>>();
        ensure_projects_active(&transaction, &source_projects).await?;
        let existing_target = project_invoice::Entity::find()
            .filter(project_invoice::Column::ProjectId.eq(&request.project_id))
            .filter(project_invoice::Column::InvoiceId.is_in(invoice_ids.clone()))
            .all(&transaction)
            .await?
            .into_iter()
            .map(|membership| membership.invoice_id)
            .collect::<HashSet<_>>();
        project_invoice::Entity::delete_many()
            .filter(project_invoice::Column::InvoiceId.is_in(invoice_ids.clone()))
            .filter(project_invoice::Column::ProjectId.ne(&request.project_id))
            .exec(&transaction)
            .await?;
        for project_id in source_projects {
            compact_project(&transaction, &project_id).await?;
            touch_project(&transaction, &project_id).await?;
        }
        compact_project(&transaction, &request.project_id).await?;
        let mut next_position = project_invoice::Entity::find()
            .filter(project_invoice::Column::ProjectId.eq(&request.project_id))
            .count(&transaction)
            .await? as i32;
        for invoice_id in invoice_ids {
            if existing_target.contains(&invoice_id) {
                continue;
            }
            project_invoice::ActiveModel {
                project_id: Set(request.project_id.clone()),
                invoice_id: Set(invoice_id),
                position: Set(next_position),
            }
            .insert(&transaction)
            .await?;
            next_position += 1;
        }
        touch_project(&transaction, &request.project_id).await?;
        transaction.commit().await?;
        Ok(AssignInvoicesResult {
            assigned: true,
            conflicts: Vec::new(),
        })
    }

    pub async fn remove_invoices_from_projects(&self, ids: Vec<String>) -> AppResult<()> {
        let invoice_ids = validate_invoice_ids(ids)?;
        let transaction = self.connection.begin().await?;
        let memberships = project_invoice::Entity::find()
            .filter(project_invoice::Column::InvoiceId.is_in(invoice_ids.clone()))
            .all(&transaction)
            .await?;
        let project_ids = memberships
            .iter()
            .map(|membership| membership.project_id.clone())
            .collect::<HashSet<_>>();
        if !project_ids.is_empty() {
            let archived = project::Entity::find()
                .filter(project::Column::Id.is_in(project_ids.iter().cloned()))
                .filter(project::Column::ArchivedAt.is_not_null())
                .count(&transaction)
                .await?;
            if archived > 0 {
                return Err(AppError::InvalidInput("归档项目不能修改成员".into()));
            }
        }
        project_invoice::Entity::delete_many()
            .filter(project_invoice::Column::InvoiceId.is_in(invoice_ids))
            .exec(&transaction)
            .await?;
        for project_id in project_ids {
            compact_project(&transaction, &project_id).await?;
            touch_project(&transaction, &project_id).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn reorder_project_invoices(
        &self,
        request: ReorderProjectInvoicesRequest,
        app_data: &Path,
    ) -> AppResult<ProjectDetail> {
        let invoice_ids = validate_invoice_ids_allow_empty(request.invoice_ids)?;
        let transaction = self.connection.begin().await?;
        ensure_project_active(&transaction, &request.project_id).await?;
        let existing = project_invoice::Entity::find()
            .filter(project_invoice::Column::ProjectId.eq(&request.project_id))
            .order_by_asc(project_invoice::Column::Position)
            .all(&transaction)
            .await?
            .into_iter()
            .map(|membership| membership.invoice_id)
            .collect::<Vec<_>>();
        if existing.len() != invoice_ids.len()
            || existing.iter().cloned().collect::<HashSet<_>>()
                != invoice_ids.iter().cloned().collect::<HashSet<_>>()
        {
            return Err(AppError::InvalidInput(
                "项目成员已变化，请刷新后重新排序".into(),
            ));
        }
        replace_project_order(&transaction, &request.project_id, &invoice_ids).await?;
        touch_project(&transaction, &request.project_id).await?;
        transaction.commit().await?;
        self.project_detail(&request.project_id, app_data).await
    }
}

fn preferred_invoice_source(app_data: &Path, row: &ProjectMemberRow) -> Option<PathBuf> {
    if !task_response_belongs_only_to_invoice(
        row.raw_response_json.as_deref()?,
        &row.raw_invoice_json,
    ) {
        return None;
    }
    let source = resolve_invoice_path(app_data, row.source_path.as_deref()?).ok()?;
    if !source.is_file() {
        return None;
    }
    if row.media_type != "application/pdf" {
        return Some(source);
    }
    if row.page_start != 1 {
        return None;
    }
    let page_count = lopdf::Document::load(&source).ok()?.get_pages().len();
    (usize::try_from(row.page_end).ok()? == page_count).then_some(source)
}

fn task_response_belongs_only_to_invoice(task_response: &str, invoice_raw: &str) -> bool {
    let Ok(task_response) = serde_json::from_str::<serde_json::Value>(task_response) else {
        return false;
    };
    let Some(response_items) = task_response
        .pointer("/Response/MixedInvoiceItems")
        .and_then(serde_json::Value::as_array)
    else {
        return false;
    };
    if response_items.is_empty()
        || response_items.iter().any(|item| {
            item.get("Code")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|code| code != "OK")
        })
    {
        return false;
    }
    let mut pages = HashSet::new();
    if response_items.iter().any(|item| {
        item.get("Page")
            .and_then(serde_json::Value::as_i64)
            .is_none_or(|page| !pages.insert(page))
    }) {
        return false;
    }
    let Ok(invoice_raw) = serde_json::from_str::<serde_json::Value>(invoice_raw) else {
        return false;
    };
    let invoice_items = match &invoice_raw {
        serde_json::Value::Array(items) => items.iter().collect::<Vec<_>>(),
        serde_json::Value::Object(_) => vec![&invoice_raw],
        _ => return false,
    };
    if invoice_items.len() != response_items.len() {
        return false;
    }
    let mut matched = vec![false; response_items.len()];
    invoice_items.into_iter().all(|invoice_item| {
        let Some(index) = response_items
            .iter()
            .enumerate()
            .position(|(index, response_item)| !matched[index] && response_item == invoice_item)
        else {
            return false;
        };
        matched[index] = true;
        true
    })
}

fn detail_from_members(
    id: String,
    name: String,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
    invoices: Vec<ProjectInvoiceMember>,
) -> ProjectDetail {
    let invoice_count = invoices.len() as i64;
    let confirmed_count = invoices.iter().filter(|invoice| invoice.confirmed).count() as i64;
    let unconfirmed_count = invoice_count - confirmed_count;
    let mut summary_counts = crate::models::MatchSummaryCounts::default();
    for invoice in invoices.iter().filter(|invoice| invoice.confirmed) {
        if let Some(status) = &invoice.summary_status {
            summary_counts.add(status);
        }
    }
    let summary_status = if summary_counts.total == 0 {
        "not_applicable"
    } else if summary_counts.failed > 0 {
        "failed"
    } else if summary_counts.pending > 0 || summary_counts.processing > 0 {
        "processing"
    } else if summary_counts.not_generated > 0 {
        "not_generated"
    } else {
        "succeeded"
    }
    .to_owned();
    let actual_amount_fen = invoices
        .iter()
        .filter(|invoice| invoice.blocking_reason.is_none())
        .filter_map(|invoice| invoice.actual_amount_fen)
        .try_fold(0_i64, |total, amount| total.checked_add(amount));
    let blocking_reason = if invoices.is_empty() {
        Some("项目暂无发票".into())
    } else {
        invoices
            .iter()
            .find_map(|invoice| invoice.blocking_reason.clone())
            .or_else(|| {
                actual_amount_fen
                    .is_none()
                    .then(|| "项目实际金额超出支持范围".into())
            })
    };
    ProjectDetail {
        id,
        name,
        invoice_count,
        confirmed_count,
        unconfirmed_count,
        summary_status,
        summary_counts,
        actual_amount_fen: actual_amount_fen.unwrap_or_default(),
        exportable: blocking_reason.is_none(),
        blocking_reason,
        created_at,
        updated_at,
        archived_at,
        invoices,
    }
}

fn summary_from_detail(detail: ProjectDetail) -> ProjectSummary {
    ProjectSummary {
        id: detail.id,
        name: detail.name,
        invoice_count: detail.invoice_count,
        confirmed_count: detail.confirmed_count,
        unconfirmed_count: detail.unconfirmed_count,
        summary_status: detail.summary_status,
        summary_counts: detail.summary_counts,
        actual_amount_fen: detail.actual_amount_fen,
        exportable: detail.exportable,
        blocking_reason: detail.blocking_reason,
        created_at: detail.created_at,
        updated_at: detail.updated_at,
        archived_at: detail.archived_at,
    }
}

fn member_blocking_reason(
    confirmed: bool,
    currency: &str,
    invoice_amount_fen: Option<i64>,
    order_amount_fen: Option<i64>,
    orders: &[ProjectOrderSummary],
    preview_available: bool,
    order_files_available: bool,
) -> Option<String> {
    if !confirmed {
        return Some("发票尚未确认订单匹配".into());
    }
    if invoice_amount_fen.is_none() {
        return Some("发票金额缺失".into());
    }
    if invoice_amount_fen.is_some_and(|value| value < 0) {
        return Some("发票金额无效".into());
    }
    if !currency.eq_ignore_ascii_case("CNY") {
        return Some("仅人民币发票可导出".into());
    }
    if orders.is_empty() {
        return Some("确认关系中没有订单".into());
    }
    if orders
        .iter()
        .any(|order| order.paid_amount_fen.is_none_or(|value| value < 0))
    {
        return Some("订单金额缺失或无效".into());
    }
    if order_amount_fen.is_none() {
        return Some("订单金额合计超出支持范围".into());
    }
    if !preview_available {
        return Some("发票独立材料缺失".into());
    }
    if !order_files_available {
        return Some("订单原文件缺失".into());
    }
    None
}

fn actual_amount(invoice_amount_fen: Option<i64>, order_amount_fen: Option<i64>) -> Option<i64> {
    invoice_amount_fen
        .zip(order_amount_fen)
        .map(|(invoice, orders)| invoice.min(orders))
}

async fn order_product_names<C: ConnectionTrait>(
    connection: &C,
    id: &str,
) -> AppResult<Vec<String>> {
    #[derive(FromQueryResult)]
    struct ProductRow {
        product_name: String,
    }
    Ok(
        ProductRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT product_name FROM order_items WHERE order_id=? ORDER BY position,id",
            [id.into()],
        ))
        .all(connection)
        .await?
        .into_iter()
        .map(|row| row.product_name)
        .collect(),
    )
}

fn validate_project_name(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 100 {
        return Err(AppError::InvalidInput(
            "项目名称长度必须为 1 到 100 个字符".into(),
        ));
    }
    Ok(value.to_owned())
}

fn validate_invoice_ids(ids: Vec<String>) -> AppResult<Vec<String>> {
    if ids.is_empty() || ids.len() > 500 {
        return Err(AppError::InvalidInput("请选择 1 到 500 张发票".into()));
    }
    validate_invoice_ids_allow_empty(ids)
}

fn validate_invoice_ids_allow_empty(ids: Vec<String>) -> AppResult<Vec<String>> {
    let mut seen = HashSet::new();
    if ids
        .iter()
        .any(|id| id.is_empty() || !seen.insert(id.clone()))
    {
        return Err(AppError::InvalidInput("发票列表包含空值或重复项".into()));
    }
    Ok(ids)
}

fn map_project_write_error(error: sea_orm::DbErr) -> AppError {
    if error
        .to_string()
        .contains("UNIQUE constraint failed: projects.name")
    {
        AppError::InvalidInput("项目名称已存在".into())
    } else {
        error.into()
    }
}

async fn ensure_project_active(transaction: &DatabaseTransaction, id: &str) -> AppResult<()> {
    let value = project::Entity::find_by_id(id)
        .one(transaction)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("项目 {id}")))?;
    if value.archived_at.is_some() {
        return Err(AppError::InvalidInput("归档项目不能修改成员".into()));
    }
    Ok(())
}

async fn ensure_projects_active(
    transaction: &DatabaseTransaction,
    project_ids: &HashSet<String>,
) -> AppResult<()> {
    if project_ids.is_empty() {
        return Ok(());
    }
    let archived = project::Entity::find()
        .filter(project::Column::Id.is_in(project_ids.iter().cloned()))
        .filter(project::Column::ArchivedAt.is_not_null())
        .count(transaction)
        .await?;
    if archived > 0 {
        return Err(AppError::InvalidInput("归档项目不能修改成员".into()));
    }
    Ok(())
}

async fn ensure_invoices_exist(
    transaction: &DatabaseTransaction,
    invoice_ids: &[String],
) -> AppResult<()> {
    let found = invoice::Entity::find()
        .filter(invoice::Column::Id.is_in(invoice_ids.to_vec()))
        .count(transaction)
        .await?;
    if found != invoice_ids.len() as u64 {
        return Err(AppError::InvalidInput(
            "部分发票已不存在，请刷新后重试".into(),
        ));
    }
    Ok(())
}

async fn current_project_conflicts(
    transaction: &DatabaseTransaction,
    invoice_ids: &[String],
    target_project_id: &str,
) -> AppResult<Vec<InvoiceProjectConflict>> {
    #[derive(FromQueryResult)]
    struct ConflictRow {
        invoice_id: String,
        source_project_id: String,
        source_project_name: String,
    }
    Ok(
        ConflictRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                "SELECT pi.invoice_id,pi.project_id source_project_id,p.name source_project_name \
             FROM project_invoices pi JOIN projects p ON p.id=pi.project_id \
             WHERE pi.project_id<>? AND pi.invoice_id IN ({}) ORDER BY pi.invoice_id",
                std::iter::repeat_n("?", invoice_ids.len())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            std::iter::once(target_project_id.into())
                .chain(invoice_ids.iter().cloned().map(Into::into))
                .collect::<Vec<sea_orm::Value>>(),
        ))
        .all(transaction)
        .await?
        .into_iter()
        .map(|row| InvoiceProjectConflict {
            invoice_id: row.invoice_id,
            source_project_id: row.source_project_id,
            source_project_name: row.source_project_name,
        })
        .collect(),
    )
}

pub(crate) async fn compact_project(
    transaction: &DatabaseTransaction,
    project_id: &str,
) -> AppResult<()> {
    let invoice_ids = project_invoice::Entity::find()
        .filter(project_invoice::Column::ProjectId.eq(project_id))
        .order_by_asc(project_invoice::Column::Position)
        .all(transaction)
        .await?
        .into_iter()
        .map(|membership| membership.invoice_id)
        .collect::<Vec<_>>();
    replace_project_order(transaction, project_id, &invoice_ids).await
}

async fn replace_project_order(
    transaction: &DatabaseTransaction,
    project_id: &str,
    invoice_ids: &[String],
) -> AppResult<()> {
    project_invoice::Entity::delete_many()
        .filter(project_invoice::Column::ProjectId.eq(project_id))
        .exec(transaction)
        .await?;
    for (position, invoice_id) in invoice_ids.iter().enumerate() {
        project_invoice::ActiveModel {
            project_id: Set(project_id.to_owned()),
            invoice_id: Set(invoice_id.clone()),
            position: Set(i32::try_from(position).unwrap_or(i32::MAX)),
        }
        .insert(transaction)
        .await?;
    }
    Ok(())
}

pub(crate) async fn touch_project(transaction: &DatabaseTransaction, id: &str) -> AppResult<()> {
    project::Entity::update_many()
        .col_expr(
            project::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(now()),
        )
        .filter(project::Column::Id.eq(id))
        .exec(transaction)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn list_projects(
    state: State<'_, AppState>,
    query: ProjectsQuery,
) -> CommandResult<ProjectsPage> {
    state
        .database
        .projects_page(query, &state.app_data_dir)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_project(
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<ProjectDetail> {
    state
        .database
        .project_detail(&request.id, &state.app_data_dir)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    request: ProjectNameRequest,
) -> CommandResult<ProjectDetail> {
    state
        .database
        .create_project(request.name, &state.app_data_dir)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_project_and_assign_invoices(
    state: State<'_, AppState>,
    request: CreateProjectAndAssignInvoicesRequest,
) -> CommandResult<ProjectDetail> {
    state
        .database
        .create_project_and_assign_invoices(request, &state.app_data_dir)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn rename_project(
    state: State<'_, AppState>,
    request: RenameProjectRequest,
) -> CommandResult<ProjectDetail> {
    state
        .database
        .rename_project(&request.id, request.name, &state.app_data_dir)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, request: IdRequest) -> CommandResult<()> {
    state
        .database
        .delete_project(&request.id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn archive_project(state: State<'_, AppState>, request: IdRequest) -> CommandResult<()> {
    state
        .database
        .set_project_archived(&request.id, true)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn unarchive_project(
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<()> {
    state
        .database
        .set_project_archived(&request.id, false)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn assign_invoices_to_project(
    state: State<'_, AppState>,
    request: AssignInvoicesToProjectRequest,
) -> CommandResult<AssignInvoicesResult> {
    state
        .database
        .assign_invoices_to_project(request)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn remove_invoices_from_projects(
    state: State<'_, AppState>,
    request: InvoiceIdsRequest,
) -> CommandResult<()> {
    state
        .database
        .remove_invoices_from_projects(request.invoice_ids)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn reorder_project_invoices(
    state: State<'_, AppState>,
    request: ReorderProjectInvoicesRequest,
) -> CommandResult<ProjectDetail> {
    state
        .database
        .reorder_project_invoices(request, &state.app_data_dir)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::invoice_match;
    use lopdf::dictionary;

    fn member_row(
        source_path: Option<String>,
        media_type: &str,
        page_start: i32,
        page_end: i32,
        raw_invoice_json: serde_json::Value,
        raw_response_json: serde_json::Value,
    ) -> ProjectMemberRow {
        ProjectMemberRow {
            id: "invoice".into(),
            position: 0,
            type_description: "发票".into(),
            invoice_code: None,
            invoice_number: None,
            invoice_date: None,
            buyer_name: None,
            buyer_tax_id: None,
            seller_name: None,
            seller_tax_id: None,
            amount_fen: None,
            tax_fen: None,
            total_fen: Some(100),
            currency: "CNY".into(),
            preview_path: "invoices/previews/invoice.png".into(),
            page_start,
            page_end,
            raw_invoice_json: raw_invoice_json.to_string(),
            source_path,
            media_type: media_type.into(),
            raw_response_json: Some(raw_response_json.to_string()),
            confirmed_source: None,
            generated_display_name: None,
            summary_status: None,
            summary_source: None,
            summary_error: None,
            summary_provider: None,
            summary_model: None,
            summary_generated_at: None,
        }
    }

    fn response(items: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({"Response": {"MixedInvoiceItems": items}})
    }

    fn write_test_pdf(path: &Path, page_count: usize) {
        let mut document = lopdf::Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_ids = (0..page_count)
            .map(|_| {
                document.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                })
            })
            .collect::<Vec<_>>();
        document.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(lopdf::Object::Reference).collect::<Vec<_>>(),
                "Count" => page_count as i64,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.save(path).unwrap();
    }

    async fn seed(database: &Database) {
        database.execute_test_statement(
            "INSERT INTO invoice_batches (id,status,created_at) VALUES ('ib','completed','2026-08-13T00:00:00Z');
             INSERT INTO invoice_tasks (id,batch_id,original_file_name,source_sha256,media_type,status,retry_count,created_at) VALUES ('it','ib','invoice.pdf','ih','application/pdf','succeeded',0,'2026-08-13T00:00:00Z');
             INSERT INTO invoices (id,task_id,type,subtype,type_description,subtype_description,total_fen,currency,preview_path,raw_invoice_json,created_at,updated_at) VALUES
               ('invoice-a','it',3,'VatInvoice','增值税发票','增值税发票',1000,'CNY','invoices/previews/a.pdf','{}','2026-08-13T00:00:00Z','2026-08-13T00:00:00Z'),
               ('invoice-b','it',3,'VatInvoice','增值税发票','增值税发票',2000,'CNY','invoices/previews/b.pdf','{}','2026-08-13T00:00:00Z','2026-08-13T00:00:00Z');"
        ).await.unwrap();
    }

    #[test]
    fn actual_amount_uses_the_lower_invoice_or_order_total() {
        assert_eq!(actual_amount(Some(1_000), Some(1_300)), Some(1_000));
        assert_eq!(actual_amount(Some(1_500), Some(1_300)), Some(1_300));
        assert_eq!(actual_amount(Some(1_300), Some(1_300)), Some(1_300));
        assert_eq!(actual_amount(None, Some(1_300)), None);
    }

    #[test]
    fn project_summary_state_uses_the_full_confirmed_member_priority() {
        let member = |id: &str, status: &str| ProjectInvoiceMember {
            id: id.into(),
            display_name: id.into(),
            summary_status: Some(status.into()),
            summary_source: None,
            summary_error: None,
            summary_provider: None,
            summary_model: None,
            summary_generated_at: None,
            position: 0,
            type_description: "发票".into(),
            invoice_code: None,
            invoice_number: None,
            invoice_date: None,
            buyer_name: None,
            buyer_tax_id: None,
            seller_name: None,
            seller_tax_id: None,
            amount_fen: Some(100),
            tax_fen: Some(0),
            invoice_amount_fen: Some(100),
            order_amount_fen: Some(100),
            actual_amount_fen: Some(100),
            currency: "CNY".into(),
            confirmed: true,
            file_available: true,
            blocking_reason: None,
            orders: vec![],
        };
        let detail = detail_from_members(
            "project".into(),
            "项目".into(),
            "now".into(),
            "now".into(),
            None,
            vec![
                member("a", "succeeded"),
                member("b", "not_generated"),
                member("c", "processing"),
                member("d", "failed"),
            ],
        );

        assert_eq!(detail.summary_status, "failed");
        assert_eq!(detail.summary_counts.total, 4);
        assert_eq!(detail.summary_counts.succeeded, 1);
        assert_eq!(detail.summary_counts.not_generated, 1);
        assert_eq!(detail.summary_counts.processing, 1);
        assert_eq!(detail.summary_counts.failed, 1);
    }

    #[test]
    fn complete_single_invoice_sources_are_preferred_without_rasterizing_pdfs() {
        let app_data = tempfile::tempdir().unwrap();
        let sources = app_data.path().join("invoices/sources");
        std::fs::create_dir_all(&sources).unwrap();
        let image_source = sources.join("invoice.png");
        std::fs::write(&image_source, b"original image").unwrap();
        let one_item = serde_json::json!({"Code": "OK", "Page": 1, "SubType": "VatInvoice"});
        let image_row = member_row(
            Some("invoices/sources/invoice.png".into()),
            "image/png",
            1,
            1,
            one_item.clone(),
            response(vec![one_item]),
        );
        assert_eq!(
            preferred_invoice_source(app_data.path(), &image_row),
            Some(image_source)
        );

        let pdf_source = sources.join("invoice.pdf");
        write_test_pdf(&pdf_source, 2);
        let first = serde_json::json!({"Code": "OK", "Page": 1, "SubType": "VatInvoice"});
        let second = serde_json::json!({"Code": "OK", "Page": 2, "SubType": "VatInvoice"});
        let pdf_row = member_row(
            Some("invoices/sources/invoice.pdf".into()),
            "application/pdf",
            1,
            2,
            serde_json::json!([first.clone(), second.clone()]),
            response(vec![first, second]),
        );
        assert_eq!(
            preferred_invoice_source(app_data.path(), &pdf_row),
            Some(pdf_source)
        );
    }

    #[test]
    fn partial_or_mixed_invoice_sources_fall_back_to_independent_previews() {
        let app_data = tempfile::tempdir().unwrap();
        let sources = app_data.path().join("invoices/sources");
        std::fs::create_dir_all(&sources).unwrap();
        let pdf_source = sources.join("invoice.pdf");
        write_test_pdf(&pdf_source, 3);
        let only_item = serde_json::json!({"Code": "OK", "Page": 2, "SubType": "VatInvoice"});
        let partial_pdf = member_row(
            Some("invoices/sources/invoice.pdf".into()),
            "application/pdf",
            2,
            2,
            only_item.clone(),
            response(vec![only_item]),
        );
        assert_eq!(
            preferred_invoice_source(app_data.path(), &partial_pdf),
            None
        );

        let image_source = sources.join("mixed.png");
        std::fs::write(image_source, b"mixed image").unwrap();
        let first = serde_json::json!({"Code": "OK", "Page": 1, "SubType": "VatInvoice"});
        let second = serde_json::json!({"Code": "OK", "Page": 1, "SubType": "TrainTicket"});
        let mixed_image = member_row(
            Some("invoices/sources/mixed.png".into()),
            "image/png",
            1,
            1,
            first.clone(),
            response(vec![first, second]),
        );
        assert_eq!(
            preferred_invoice_source(app_data.path(), &mixed_image),
            None
        );
    }

    #[test]
    fn export_eligibility_reports_each_blocking_category() {
        let order = ProjectOrderSummary {
            id: "order".into(),
            external_order_number: None,
            paid_amount_fen: Some(1_000),
            product_names: Vec::new(),
        };
        assert_eq!(
            member_blocking_reason(
                false,
                "CNY",
                Some(1_000),
                Some(1_000),
                std::slice::from_ref(&order),
                true,
                true
            ),
            Some("发票尚未确认订单匹配".into())
        );
        assert_eq!(
            member_blocking_reason(
                true,
                "CNY",
                None,
                Some(1_000),
                std::slice::from_ref(&order),
                true,
                true,
            ),
            Some("发票金额缺失".into())
        );
        assert_eq!(
            member_blocking_reason(
                true,
                "CNY",
                Some(-1),
                Some(1_000),
                std::slice::from_ref(&order),
                true,
                true
            ),
            Some("发票金额无效".into())
        );
        assert_eq!(
            member_blocking_reason(
                true,
                "USD",
                Some(1_000),
                Some(1_000),
                std::slice::from_ref(&order),
                true,
                true
            ),
            Some("仅人民币发票可导出".into())
        );
        assert_eq!(
            member_blocking_reason(true, "CNY", Some(1_000), Some(0), &[], true, true),
            Some("确认关系中没有订单".into())
        );
        let mut missing_money = order.clone();
        missing_money.paid_amount_fen = None;
        assert_eq!(
            member_blocking_reason(true, "CNY", Some(1_000), None, &[missing_money], true, true),
            Some("订单金额缺失或无效".into())
        );
        assert_eq!(
            member_blocking_reason(
                true,
                "CNY",
                Some(1_000),
                Some(1_000),
                std::slice::from_ref(&order),
                false,
                true
            ),
            Some("发票独立材料缺失".into())
        );
        assert_eq!(
            member_blocking_reason(
                true,
                "CNY",
                Some(1_000),
                Some(1_000),
                std::slice::from_ref(&order),
                true,
                false
            ),
            Some("订单原文件缺失".into())
        );
        assert_eq!(
            member_blocking_reason(true, "CNY", Some(1_000), Some(1_000), &[order], true, true),
            None
        );
        assert_eq!(
            member_blocking_reason(
                true,
                "CNY",
                Some(1_000),
                None,
                &[ProjectOrderSummary {
                    id: "order".into(),
                    external_order_number: None,
                    paid_amount_fen: Some(i64::MAX),
                    product_names: Vec::new(),
                }],
                true,
                true
            ),
            Some("订单金额合计超出支持范围".into())
        );
        assert_eq!(
            detail_from_members(
                "id".into(),
                "空项目".into(),
                "now".into(),
                "now".into(),
                None,
                vec![]
            )
            .blocking_reason,
            Some("项目暂无发票".into())
        );
    }

    #[tokio::test]
    async fn project_names_are_trimmed_and_unique() {
        let database = Database::in_memory().await.unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let project = database
            .create_project("  唯一项目  ".into(), app_data.path())
            .await
            .unwrap();
        assert_eq!(project.name, "唯一项目");
        assert!(
            database
                .create_project("唯一项目".into(), app_data.path())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn archive_views_preserve_members_and_block_mutation() {
        let database = Database::in_memory().await.unwrap();
        seed(&database).await;
        let app_data = tempfile::tempdir().unwrap();
        let project = database
            .create_project("待归档".into(), app_data.path())
            .await
            .unwrap();
        database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into()],
                project_id: project.id.clone(),
                accept_reassignment: false,
            })
            .await
            .unwrap();

        database
            .set_project_archived(&project.id, true)
            .await
            .unwrap();
        let active = database
            .projects_page(
                ProjectsQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    archived: Some(false),
                },
                app_data.path(),
            )
            .await
            .unwrap();
        let archived = database
            .projects_page(
                ProjectsQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    archived: Some(true),
                },
                app_data.path(),
            )
            .await
            .unwrap();
        assert!(active.items.is_empty());
        assert_eq!(archived.items[0].id, project.id);
        assert_eq!(
            database
                .project_detail(&project.id, app_data.path())
                .await
                .unwrap()
                .invoice_count,
            1
        );
        assert!(
            database
                .rename_project(&project.id, "新名称".into(), app_data.path())
                .await
                .is_err()
        );
        assert!(
            database
                .reorder_project_invoices(
                    ReorderProjectInvoicesRequest {
                        project_id: project.id.clone(),
                        invoice_ids: vec!["invoice-a".into()]
                    },
                    app_data.path()
                )
                .await
                .is_err()
        );
        assert!(
            database
                .remove_invoices_from_projects(vec!["invoice-a".into()])
                .await
                .is_err()
        );
        let active_target = database
            .create_project("进行中目标".into(), app_data.path())
            .await
            .unwrap();
        assert!(
            database
                .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                    invoice_ids: vec!["invoice-a".into()],
                    project_id: active_target.id,
                    accept_reassignment: true,
                })
                .await
                .is_err()
        );

        database
            .set_project_archived(&project.id, false)
            .await
            .unwrap();
        assert!(
            database
                .projects_page(
                    ProjectsQuery {
                        page: None,
                        page_size: None,
                        search: None,
                        archived: Some(false)
                    },
                    app_data.path()
                )
                .await
                .unwrap()
                .items
                .iter()
                .any(|item| item.id == project.id)
        );
    }

    #[tokio::test]
    async fn create_and_assign_rolls_back_when_any_invoice_is_occupied() {
        let database = Database::in_memory().await.unwrap();
        seed(&database).await;
        let app_data = tempfile::tempdir().unwrap();
        let existing = database
            .create_project("现有项目".into(), app_data.path())
            .await
            .unwrap();
        database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into()],
                project_id: existing.id,
                accept_reassignment: false,
            })
            .await
            .unwrap();

        assert!(
            database
                .create_project_and_assign_invoices(
                    CreateProjectAndAssignInvoicesRequest {
                        name: "不应保留".into(),
                        invoice_ids: vec!["invoice-a".into(), "invoice-b".into()]
                    },
                    app_data.path()
                )
                .await
                .is_err()
        );
        assert!(
            project::Entity::find()
                .filter(project::Column::Name.eq("不应保留"))
                .one(&database.connection)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn assignment_previews_then_atomically_reassigns_members() {
        let database = Database::in_memory().await.unwrap();
        seed(&database).await;
        let app_data = tempfile::tempdir().unwrap();
        let first = database
            .create_project("项目 A".into(), app_data.path())
            .await
            .unwrap();
        let second = database
            .create_project("项目 B".into(), app_data.path())
            .await
            .unwrap();
        database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into(), "invoice-b".into()],
                project_id: first.id.clone(),
                accept_reassignment: false,
            })
            .await
            .unwrap();
        let preview = database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into()],
                project_id: second.id.clone(),
                accept_reassignment: false,
            })
            .await
            .unwrap();
        assert!(!preview.assigned);
        assert_eq!(preview.conflicts.len(), 1);
        database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into()],
                project_id: second.id.clone(),
                accept_reassignment: true,
            })
            .await
            .unwrap();
        let first_detail = database
            .project_detail(&first.id, app_data.path())
            .await
            .unwrap();
        let second_detail = database
            .project_detail(&second.id, app_data.path())
            .await
            .unwrap();
        assert_eq!(
            first_detail
                .invoices
                .iter()
                .map(|value| value.id.as_str())
                .collect::<Vec<_>>(),
            vec!["invoice-b"]
        );
        assert_eq!(first_detail.invoices[0].position, 0);
        assert_eq!(second_detail.invoices[0].id, "invoice-a");
    }

    #[tokio::test]
    async fn reorder_requires_the_current_complete_member_set() {
        let database = Database::in_memory().await.unwrap();
        seed(&database).await;
        let app_data = tempfile::tempdir().unwrap();
        let project = database
            .create_project("排序项目".into(), app_data.path())
            .await
            .unwrap();
        database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into(), "invoice-b".into()],
                project_id: project.id.clone(),
                accept_reassignment: false,
            })
            .await
            .unwrap();
        let reordered = database
            .reorder_project_invoices(
                ReorderProjectInvoicesRequest {
                    project_id: project.id.clone(),
                    invoice_ids: vec!["invoice-b".into(), "invoice-a".into()],
                },
                app_data.path(),
            )
            .await
            .unwrap();
        assert_eq!(
            reordered
                .invoices
                .iter()
                .map(|value| value.id.as_str())
                .collect::<Vec<_>>(),
            vec!["invoice-b", "invoice-a"]
        );
        assert!(
            database
                .reorder_project_invoices(
                    ReorderProjectInvoicesRequest {
                        project_id: project.id,
                        invoice_ids: vec!["invoice-a".into()]
                    },
                    app_data.path()
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn bulk_removal_compacts_members_and_preserves_invoices_and_matches() {
        let database = Database::in_memory().await.unwrap();
        seed(&database).await;
        database.execute_test_statement(
            "INSERT INTO invoices (id,task_id,type,subtype,type_description,subtype_description,total_fen,currency,preview_path,raw_invoice_json,created_at,updated_at) VALUES
               ('invoice-c','it',3,'VatInvoice','增值税发票','增值税发票',3000,'CNY','invoices/previews/c.pdf','{}','2026-08-13T00:00:00Z','2026-08-13T00:00:00Z');
             INSERT INTO import_batches (id,platform,status,created_at) VALUES
               ('order-batch','taobao','completed','2026-08-13T00:00:00Z');
             INSERT INTO import_tasks (id,batch_id,original_file_name,image_sha256,status,retry_count,created_at) VALUES
               ('order-a','order-batch','order-a.png','order-hash-a','succeeded',0,'2026-08-13T00:00:00Z'),
               ('order-b','order-batch','order-b.png','order-hash-b','succeeded',0,'2026-08-13T00:00:00Z'),
               ('order-c','order-batch','order-c.png','order-hash-c','succeeded',0,'2026-08-13T00:00:00Z');
             INSERT INTO orders (id,platform,external_order_number,paid_amount_fen,raw_ocr_json,created_at,updated_at) VALUES
               ('order-a','taobao','ORDER-A',1000,'{}','2026-08-13T00:00:00Z','2026-08-13T00:00:00Z'),
               ('order-b','taobao','ORDER-B',2000,'{}','2026-08-13T00:00:00Z','2026-08-13T00:00:00Z'),
               ('order-c','taobao','ORDER-C',3000,'{}','2026-08-13T00:00:00Z','2026-08-13T00:00:00Z');
             INSERT INTO invoice_matches (invoice_id,source,confirmed_at) VALUES
               ('invoice-a','automatic','2026-08-13T00:00:00Z'),
               ('invoice-b','automatic','2026-08-13T00:00:00Z'),
               ('invoice-c','automatic','2026-08-13T00:00:00Z');
             INSERT INTO invoice_match_orders (invoice_id,order_id,position) VALUES
               ('invoice-a','order-a',0),('invoice-b','order-b',0),('invoice-c','order-c',0);",
        ).await.unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let project = database
            .create_project("批量移除成员".into(), app_data.path())
            .await
            .unwrap();
        database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into(), "invoice-b".into(), "invoice-c".into()],
                project_id: project.id.clone(),
                accept_reassignment: false,
            })
            .await
            .unwrap();

        database
            .remove_invoices_from_projects(vec!["invoice-a".into(), "invoice-b".into()])
            .await
            .unwrap();

        let memberships = project_invoice::Entity::find()
            .filter(project_invoice::Column::ProjectId.eq(&project.id))
            .order_by_asc(project_invoice::Column::Position)
            .all(&database.connection)
            .await
            .unwrap();
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0].invoice_id, "invoice-c");
        assert_eq!(memberships[0].position, 0);
        assert_eq!(
            invoice::Entity::find()
                .count(&database.connection)
                .await
                .unwrap(),
            3
        );
        assert_eq!(
            invoice_match::Entity::find()
                .count(&database.connection)
                .await
                .unwrap(),
            3
        );
    }

    #[tokio::test]
    async fn invoice_deletion_compacts_the_remaining_project_members() {
        let database = Database::in_memory().await.unwrap();
        seed(&database).await;
        let app_data = tempfile::tempdir().unwrap();
        let project = database
            .create_project("删除项目成员".into(), app_data.path())
            .await
            .unwrap();
        database
            .assign_invoices_to_project(AssignInvoicesToProjectRequest {
                invoice_ids: vec!["invoice-a".into(), "invoice-b".into()],
                project_id: project.id.clone(),
                accept_reassignment: false,
            })
            .await
            .unwrap();

        database.delete_invoice_record("invoice-a").await.unwrap();

        let detail = database
            .project_detail(&project.id, app_data.path())
            .await
            .unwrap();
        assert_eq!(detail.invoices.len(), 1);
        assert_eq!(detail.invoices[0].id, "invoice-b");
        assert_eq!(detail.invoices[0].position, 0);
    }
}
