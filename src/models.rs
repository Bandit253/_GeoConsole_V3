use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Dataset Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub id: Uuid,
    pub name: String,
    pub table_name: String,
    pub geometry_column: String,
    pub geometry_type: GeometryType,
    pub srid: i32,
    pub feature_count: i64,
    pub bounds: Option<Bounds>,
    pub columns: Vec<ColumnInfo>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GeometryType {
    Point,
    LineString,
    Polygon,
    MultiPoint,
    MultiLineString,
    MultiPolygon,
    GeometryCollection,
    Unknown,
}

impl From<&str> for GeometryType {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "POINT" => GeometryType::Point,
            "LINESTRING" => GeometryType::LineString,
            "POLYGON" => GeometryType::Polygon,
            "MULTIPOINT" => GeometryType::MultiPoint,
            "MULTILINESTRING" => GeometryType::MultiLineString,
            "MULTIPOLYGON" => GeometryType::MultiPolygon,
            "GEOMETRYCOLLECTION" => GeometryType::GeometryCollection,
            _ => GeometryType::Unknown,
        }
    }
}

// ============================================================================
// Map Studio Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConfig {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub basemap: String,
    pub view: MapView,
    pub layers: Vec<MapLayer>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapView {
    pub center: [f64; 2], // [lon, lat]
    pub zoom: f64,
    pub bearing: f64,
    pub pitch: f64,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            zoom: 2.0,
            bearing: 0.0,
            pitch: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapLayer {
    pub id: Uuid,
    pub name: String,
    pub layer_type: LayerType,
    pub visible: bool,
    pub opacity: f64,
    pub z_index: i32,
    pub dataset_id: Option<Uuid>,
    pub style: LayerStyle,
    pub label_config: Option<LabelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerType {
    Vector,
    Raster,
    GeoJson,
    #[serde(rename = "deck.gl")]
    DeckGl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerStyle {
    pub style_type: StyleType,
    pub fill_color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub opacity: Option<f64>,
    pub radius: Option<f64>,
    // Data-driven styling
    pub property: Option<String>,
    pub breaks: Option<Vec<f64>>,
    pub colors: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StyleType {
    Simple,
    Categorized,
    Graduated,
    Heatmap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelConfig {
    pub enabled: bool,
    pub field: String,
    pub font_size: f64,
    pub font_color: String,
    pub halo_color: Option<String>,
    pub halo_width: Option<f64>,
}

// ============================================================================
// Routing Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub locations: Vec<[f64; 2]>, // [[lon, lat], ...]
    pub costing: String,          // "auto", "bicycle", "pedestrian"
    pub units: Option<String>,    // "kilometers", "miles"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsochroneRequest {
    pub locations: Vec<[f64; 2]>,
    pub costing: String,
    pub contours: Vec<IsochroneContour>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsochroneContour {
    pub time: f64,      // minutes
    pub color: String,  // hex color
}

// ============================================================================
// API Request/Response Models
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMapRequest {
    pub name: String,
    pub description: Option<String>,
    pub basemap: Option<String>,
    pub view: Option<MapView>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMapRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub basemap: Option<String>,
    pub view: Option<MapView>,
    pub layers: Option<Vec<MapLayer>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatasetListResponse {
    pub datasets: Vec<Dataset>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MapListResponse {
    pub maps: Vec<MapConfig>,
    pub total: usize,
}
