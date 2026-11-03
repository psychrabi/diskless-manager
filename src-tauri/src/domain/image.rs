use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ImageVersionName {
    pub base_name: String,
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageVersionSummary {
    pub id: String,
    pub name: String,
    pub base_name: String,
    pub version: String,
    pub image_asset_id: String,
    pub parent_version_id: Option<String>,
    pub os_type: String,
    pub source_description: Option<String>,
    pub comment: Option<String>,
    pub is_pinned: bool,
    pub is_locked: bool,
    pub ordering: i64,
    pub published_by: Option<String>,
    pub published_at: String,
    pub last_used_at: Option<String>,
    pub last_used_by_client_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::persistence::repositories::image_version::ImageVersionRow>
    for ImageVersionSummary
{
    fn from(row: crate::persistence::repositories::image_version::ImageVersionRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            base_name: row.base_name,
            version: row.version,
            image_asset_id: row.image_asset_id,
            parent_version_id: row.parent_version_id,
            os_type: row.os_type,
            source_description: row.source_description,
            comment: row.comment,
            is_pinned: row.is_pinned,
            is_locked: row.is_locked,
            ordering: row.ordering,
            published_by: row.published_by,
            published_at: row.published_at,
            last_used_at: row.last_used_at,
            last_used_by_client_id: row.last_used_by_client_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageVersionView {
    pub summary: ImageVersionSummary,
    pub current_asset: Option<crate::core::image::Image>,
}

pub type ImageVersionListResult = Vec<ImageVersionSummary>;
pub type ImageVersionOption = Option<ImageVersionSummary>;
