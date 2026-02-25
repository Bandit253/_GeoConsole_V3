use duckdb::Connection;
use duckdb::arrow::ipc::writer::StreamWriter;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use uuid::Uuid;
use chrono::Utc;

use crate::error::{AppError, AppResult};
use crate::models::{Dataset, ColumnInfo, Bounds, GeometryType, MapConfig, MapView};

/// Validate a SQL identifier (table name, column name) to prevent injection.
/// Only allows alphanumeric chars, underscores, and dots (for schema-qualified names).
fn validate_identifier(name: &str) -> AppResult<()> {
    if name.is_empty() {
        return Err(AppError::BadRequest("Empty identifier".to_string()));
    }
    if name.len() > 128 {
        return Err(AppError::BadRequest("Identifier too long".to_string()));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
        return Err(AppError::BadRequest(format!(
            "Invalid identifier '{}': only alphanumeric, underscore, and dot allowed", name
        )));
    }
    Ok(())
}

/// Double-quote a SQL identifier after validation.
fn quote_ident(name: &str) -> AppResult<String> {
    validate_identifier(name)?;
    Ok(format!("\"{}\"", name))
}

/// Validate and canonicalize a file path, ensuring it stays within the given data directory.
fn validate_file_path(file_path: &str, data_dir: &std::path::Path) -> AppResult<String> {
    let path = std::path::Path::new(file_path);
    let canonical = path.canonicalize().map_err(|e| {
        AppError::BadRequest(format!("Invalid file path '{}': {}", file_path, e))
    })?;
    let allowed_dir = data_dir.canonicalize().map_err(|e| {
        AppError::Internal(format!("Cannot resolve data directory: {}", e))
    })?;
    if !canonical.starts_with(&allowed_dir) {
        return Err(AppError::BadRequest(format!(
            "File path '{}' is outside the allowed data directory", file_path
        )));
    }
    let mut result = canonical.to_string_lossy().to_string();
    // Strip Windows extended-length path prefix (\\?\) — GDAL doesn't understand it
    if result.starts_with(r"\\?\") {
        result = result[4..].to_string();
    }
    Ok(result.replace('\\', "/").replace('\'', "''"))
}

/// Validate bbox floats are finite and in reasonable range.
fn validate_bbox(minx: f64, miny: f64, maxx: f64, maxy: f64) -> AppResult<()> {
    for v in [minx, miny, maxx, maxy] {
        if !v.is_finite() {
            return Err(AppError::BadRequest("bbox values must be finite numbers".to_string()));
        }
    }
    if minx >= maxx || miny >= maxy {
        return Err(AppError::BadRequest("bbox min must be less than max".to_string()));
    }
    Ok(())
}

pub struct DuckDbManager {
    /// Single connection protected by Mutex.
    /// DuckDB's Connection is !Sync (contains RefCell), so RwLock won't work.
    /// DuckDB on Windows also doesn't allow multiple Connection::open on the same file.
    /// A single Mutex<Connection> is the correct approach — DuckDB handles internal
    /// read concurrency within a connection.
    conn: Arc<Mutex<Connection>>,
    /// Dataset metadata (persisted in DuckDB metadata tables)
    datasets: Arc<RwLock<HashMap<Uuid, Dataset>>>,
    maps: Arc<RwLock<HashMap<Uuid, MapConfig>>>,
    /// Path to the data directory
    pub db_path: PathBuf,
}

fn lock_err<T>(e: std::sync::PoisonError<T>) -> AppError {
    AppError::Internal(e.to_string())
}

impl DuckDbManager {
    pub fn new() -> AppResult<Self> {
        Self::new_with_data_dir(PathBuf::from("data"), true)
    }

    /// Create an in-memory DuckDbManager (useful for testing).
    /// Uses a file-system directory for uploaded files but :memory: for DuckDB itself.
    /// No read pool is created — all reads go through the write connection.
    pub fn new_in_memory(data_dir: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&data_dir).map_err(|e| {
            AppError::Internal(format!("Failed to create data directory: {}", e))
        })?;

        let conn = Connection::open_in_memory()?;
        conn.execute_batch("LOAD spatial; LOAD parquet;")?;

        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS _meta_datasets (
                id VARCHAR PRIMARY KEY,
                name VARCHAR NOT NULL,
                table_name VARCHAR NOT NULL,
                geometry_column VARCHAR NOT NULL,
                geometry_type VARCHAR NOT NULL,
                srid INTEGER NOT NULL DEFAULT 4326,
                feature_count BIGINT NOT NULL DEFAULT 0,
                bounds_min_x DOUBLE,
                bounds_min_y DOUBLE,
                bounds_max_x DOUBLE,
                bounds_max_y DOUBLE,
                columns_json VARCHAR,
                created_at VARCHAR NOT NULL,
                updated_at VARCHAR NOT NULL
            );
            CREATE TABLE IF NOT EXISTS _meta_maps (
                id VARCHAR PRIMARY KEY,
                config_json VARCHAR NOT NULL,
                created_at VARCHAR NOT NULL,
                updated_at VARCHAR NOT NULL
            );
        "#)?;

        let manager = Self {
            conn: Arc::new(Mutex::new(conn)),
            datasets: Arc::new(RwLock::new(HashMap::new())),
            maps: Arc::new(RwLock::new(HashMap::new())),
            db_path: data_dir,
        };

        Ok(manager)
    }

    /// Create a DuckDbManager with a custom data directory.
    /// When `install_extensions` is true, runs INSTALL (downloads/caches extensions globally).
    /// Set to false if extensions are already installed (e.g. in tests) to avoid lock contention.
    pub fn new_with_data_dir(data_dir: PathBuf, install_extensions: bool) -> AppResult<Self> {
        std::fs::create_dir_all(&data_dir).map_err(|e| {
            AppError::Internal(format!("Failed to create data directory: {}", e))
        })?;
        let db_path = data_dir.join("geoconsole.duckdb");
        let is_new = !db_path.exists();

        let conn = Connection::open(&db_path)?;
        tracing::info!("DuckDB file: {} (new={})", db_path.display(), is_new);

        // Set DuckDB home directory to data directory (for extensions)
        let home_dir = data_dir.canonicalize()
            .unwrap_or_else(|_| data_dir.clone())
            .to_string_lossy()
            .to_string();
        let set_home_sql = format!("SET home_directory='{}'", home_dir.replace('\'', "''"));
        conn.execute_batch(&set_home_sql)?;
        tracing::info!("DuckDB home directory: {}", home_dir);

        // Load extensions
        if conn.execute_batch("LOAD spatial; LOAD parquet;").is_err() {
            if install_extensions {
                conn.execute_batch("INSTALL spatial; INSTALL parquet;")?;
                conn.execute_batch("LOAD spatial; LOAD parquet;")?;
            } else {
                return Err(AppError::Internal("Failed to load DuckDB extensions".to_string()));
            }
        }
        tracing::info!("DuckDB spatial and parquet extensions loaded");

        // Create metadata tables
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS _meta_datasets (
                id VARCHAR PRIMARY KEY,
                name VARCHAR NOT NULL,
                table_name VARCHAR NOT NULL,
                geometry_column VARCHAR NOT NULL,
                geometry_type VARCHAR NOT NULL,
                srid INTEGER NOT NULL DEFAULT 4326,
                feature_count BIGINT NOT NULL DEFAULT 0,
                bounds_min_x DOUBLE,
                bounds_min_y DOUBLE,
                bounds_max_x DOUBLE,
                bounds_max_y DOUBLE,
                columns_json VARCHAR,
                created_at VARCHAR NOT NULL,
                updated_at VARCHAR NOT NULL
            );
            CREATE TABLE IF NOT EXISTS _meta_maps (
                id VARCHAR PRIMARY KEY,
                config_json VARCHAR NOT NULL,
                created_at VARCHAR NOT NULL,
                updated_at VARCHAR NOT NULL
            );
        "#)?;
        tracing::info!("Metadata tables ready");

        // Restore metadata using the connection directly (before wrapping in Mutex)
        let datasets = Arc::new(RwLock::new(HashMap::new()));
        let maps = Arc::new(RwLock::new(HashMap::new()));
        match Self::restore_metadata_from(&conn, &datasets, &maps) {
            Ok(()) => {},
            Err(e) => {
                tracing::warn!("Failed to restore metadata (starting fresh): {}", e);
            }
        }

        let manager = Self {
            conn: Arc::new(Mutex::new(conn)),
            datasets,
            maps,
            db_path: data_dir,
        };

        Ok(manager)
    }

    /// Acquire the connection lock.
    fn read_conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(lock_err)
    }

    /// Acquire the connection lock (same as read_conn — single Mutex for all access).
    fn write_conn(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(lock_err)
    }

    /// Restore dataset and map metadata from DuckDB metadata tables on startup.
    /// Takes references directly (called before Mutex wrapping during init).
    fn restore_metadata_from(
        conn: &Connection,
        datasets: &Arc<RwLock<HashMap<Uuid, Dataset>>>,
        maps: &Arc<RwLock<HashMap<Uuid, MapConfig>>>,
    ) -> AppResult<()> {
        tracing::info!("Restoring metadata...");
        let mut stmt = conn.prepare(
            "SELECT id, name, table_name, geometry_column, geometry_type, srid, feature_count, \
             bounds_min_x, bounds_min_y, bounds_max_x, bounds_max_y, columns_json, created_at, updated_at \
             FROM _meta_datasets"
        )?;
        let rows: Vec<(String, String, String, String, String, i32, i64, 
                       Option<f64>, Option<f64>, Option<f64>, Option<f64>, 
                       Option<String>, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                    row.get(4)?, row.get(5)?, row.get(6)?,
                    row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?,
                    row.get(11)?, row.get(12)?, row.get(13)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut ds = datasets.write().map_err(lock_err)?;
        for (id_str, name, table_name, geom_col, geom_type, srid, count,
             bminx, bminy, bmaxx, bmaxy, cols_json, created, updated) in rows
        {
            let id = Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4());
            
            // Verify the data table still exists
            if validate_identifier(&table_name).is_err() {
                tracing::warn!("Invalid table name {}, skipping dataset {}", table_name, id);
                continue;
            }
            let check_sql = format!("SELECT 1 FROM {} LIMIT 0", table_name);
            if conn.execute(&check_sql, []).is_err() {
                tracing::warn!("Data table {} missing, skipping dataset {}", table_name, id);
                continue;
            }

            let bounds = match (bminx, bminy, bmaxx, bmaxy) {
                (Some(x1), Some(y1), Some(x2), Some(y2)) => Some(Bounds { min_x: x1, min_y: y1, max_x: x2, max_y: y2 }),
                _ => None,
            };
            let columns: Vec<ColumnInfo> = cols_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default();
            let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let dataset = Dataset {
                id, name, table_name, geometry_column: geom_col,
                geometry_type: GeometryType::from(geom_type.as_str()),
                srid, feature_count: count, bounds, columns,
                created_at, updated_at,
            };
            tracing::info!("Restored dataset: {} ({}, {} features)", dataset.name, dataset.id, dataset.feature_count);
            ds.insert(id, dataset);
        }
        drop(ds);

        // Restore maps
        let mut stmt = conn.prepare("SELECT id, config_json FROM _meta_maps")?;
        let map_rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut m = maps.write().map_err(lock_err)?;
        for (id_str, config_json) in map_rows {
            if let Ok(map) = serde_json::from_str::<MapConfig>(&config_json) {
                let id = Uuid::parse_str(&id_str).unwrap_or(map.id);
                tracing::info!("Restored map: {} ({})", map.name, id);
                m.insert(id, map);
            }
        }
        drop(m);

        let ds_count = datasets.read().map_err(lock_err)?.len();
        let map_count = maps.read().map_err(lock_err)?.len();
        tracing::info!("Restored {} datasets and {} maps from disk", ds_count, map_count);
        Ok(())
    }

    /// Persist a dataset's metadata to the _meta_datasets table
    fn persist_dataset_meta(&self, conn: &Connection, dataset: &Dataset) -> AppResult<()> {
        let cols_json = serde_json::to_string(&dataset.columns).unwrap_or_default();
        let bounds = dataset.bounds.as_ref();
        conn.execute(
            "INSERT OR REPLACE INTO _meta_datasets \
             (id, name, table_name, geometry_column, geometry_type, srid, feature_count, \
              bounds_min_x, bounds_min_y, bounds_max_x, bounds_max_y, columns_json, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                dataset.id.to_string(),
                dataset.name,
                dataset.table_name,
                dataset.geometry_column,
                format!("{:?}", dataset.geometry_type),
                dataset.srid,
                dataset.feature_count,
                bounds.map(|b| b.min_x),
                bounds.map(|b| b.min_y),
                bounds.map(|b| b.max_x),
                bounds.map(|b| b.max_y),
                cols_json,
                dataset.created_at.to_rfc3339(),
                dataset.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Persist a map config to the _meta_maps table
    fn persist_map_meta(&self, conn: &Connection, map: &MapConfig) -> AppResult<()> {
        let config_json = serde_json::to_string(map)
            .map_err(|e| AppError::Internal(format!("Failed to serialize map: {}", e)))?;
        conn.execute(
            "INSERT OR REPLACE INTO _meta_maps (id, config_json, created_at, updated_at) VALUES (?, ?, ?, ?)",
            duckdb::params![
                map.id.to_string(),
                config_json,
                map.created_at.to_rfc3339(),
                map.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Load a spatial file into DuckDB (auto-detects format)
    pub fn load_spatial_file(&self, file_path: &str, name: &str) -> AppResult<Dataset> {
        // Validate and canonicalize the file path (restricts to data directory)
        let safe_path = validate_file_path(file_path, &self.db_path)?;
        let path_lower = file_path.to_lowercase();
        
        // Parquet uses read_parquet(), everything else uses ST_Read()
        let read_fn = if path_lower.ends_with(".parquet") || path_lower.ends_with(".geoparquet") {
            "read_parquet"
        } else if path_lower.ends_with(".gpkg")
            || path_lower.ends_with(".shp")
            || path_lower.ends_with(".geojson")
            || path_lower.ends_with(".json")
            || path_lower.ends_with(".kml")
        {
            "ST_Read"
        } else {
            return Err(AppError::BadRequest(
                "Unsupported file format. Supported: .parquet, .geoparquet, .gpkg, .shp, .geojson, .json, .kml".to_string()
            ));
        };

        let conn = self.write_conn()?;
        
        let id = Uuid::new_v4();
        let table_name = format!("dataset_{}", id.to_string().replace("-", "_"));
        validate_identifier(&table_name)?;

        let create_sql = format!(
            "CREATE TABLE {} AS SELECT * FROM {}('{}')",
            table_name, read_fn, safe_path
        );
        conn.execute(&create_sql, [])?;
        tracing::info!("Loaded {} via {}() -> {}", file_path, read_fn, table_name);

        self.finalize_dataset(&conn, id, name, table_name)
    }

    /// Finalize dataset after loading - extract metadata
    fn finalize_dataset(&self, conn: &Connection, id: Uuid, name: &str, table_name: String) -> AppResult<Dataset> {
        validate_identifier(&table_name)?;
        let geometry_column = self.detect_geometry_column(conn, &table_name)?;
        let geom_qi = quote_ident(&geometry_column)?;
        let columns = self.get_column_info(conn, &table_name)?;
        
        let feature_count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", table_name),
            [],
            |row| row.get(0),
        )?;

        let geometry_type = self.detect_geometry_type(conn, &table_name, &geometry_column)?;
        let srid = self.detect_srid(conn, &table_name, &geometry_column);
        
        // If not already WGS84, reproject the geometry column in-place
        if srid != 4326 {
            let transform_sql = format!(
                "UPDATE {} SET {} = ST_Transform({}, 'EPSG:{}', 'EPSG:4326')",
                table_name, geom_qi, geom_qi, srid
            );
            tracing::info!("Reprojecting {} from EPSG:{} to EPSG:4326", table_name, srid);
            if let Err(e) = conn.execute(&transform_sql, []) {
                tracing::warn!("Failed to reproject with SRID {}: {}. Trying auto-detect...", srid, e);
                // Fallback: try without explicit source CRS (DuckDB may auto-detect)
                let fallback_sql = format!(
                    "UPDATE {} SET {} = ST_Transform({}, 'EPSG:4326')",
                    table_name, geom_qi, geom_qi
                );
                if let Err(e2) = conn.execute(&fallback_sql, []) {
                    tracing::warn!("Reprojection fallback also failed: {}", e2);
                }
            }
        }

        // Materialize per-row bounding box columns for fast spatial filtering.
        // DuckDB's columnar zone maps turn bbox range predicates into indexed scans.
        let bbox_sql = format!(
            "ALTER TABLE {} ADD COLUMN IF NOT EXISTS _bbox_xmin DOUBLE; \
             ALTER TABLE {} ADD COLUMN IF NOT EXISTS _bbox_ymin DOUBLE; \
             ALTER TABLE {} ADD COLUMN IF NOT EXISTS _bbox_xmax DOUBLE; \
             ALTER TABLE {} ADD COLUMN IF NOT EXISTS _bbox_ymax DOUBLE;",
            table_name, table_name, table_name, table_name
        );
        conn.execute_batch(&bbox_sql)?;
        let bbox_update_sql = format!(
            "UPDATE {} SET \
             _bbox_xmin = ST_XMin(ST_Envelope({})), \
             _bbox_ymin = ST_YMin(ST_Envelope({})), \
             _bbox_xmax = ST_XMax(ST_Envelope({})), \
             _bbox_ymax = ST_YMax(ST_Envelope({}))",
            table_name, geom_qi, geom_qi, geom_qi, geom_qi
        );
        conn.execute(&bbox_update_sql, [])?;
        tracing::info!("Materialized bbox columns for {}", table_name);

        let bounds = self.get_bounds(conn, &table_name, &geometry_column)?;

        let now = Utc::now();
        let dataset = Dataset {
            id,
            name: name.to_string(),
            table_name,
            geometry_column,
            geometry_type,
            srid: 4326,
            feature_count,
            bounds: Some(bounds),
            columns,
            created_at: now,
            updated_at: now,
        };

        // Persist metadata to disk
        self.persist_dataset_meta(conn, &dataset)?;

        self.datasets.write().map_err(lock_err)?
            .insert(id, dataset.clone());

        Ok(dataset)
    }

    fn detect_geometry_column(&self, conn: &Connection, table_name: &str) -> AppResult<String> {
        validate_identifier(table_name)?;
        let candidates = ["geometry", "geom", "wkb_geometry", "the_geom", "shape"];
        
        let mut stmt = conn.prepare(
            "SELECT column_name FROM information_schema.columns WHERE table_name = ?",
        )?;
        let column_names: Vec<String> = stmt
            .query_map(duckdb::params![table_name], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for candidate in candidates {
            if let Some(actual) = column_names.iter().find(|c| c.to_lowercase() == candidate) {
                return Ok(actual.clone());
            }
        }

        // Try to find any geometry column by type
        for col in &column_names {
            let col_qi = quote_ident(col)?;
            let check_sql = format!(
                "SELECT ST_GeometryType({}) FROM {} LIMIT 1",
                col_qi, table_name
            );
            if conn.query_row(&check_sql, [], |_| Ok(())).is_ok() {
                return Ok(col.clone());
            }
        }

        Err(AppError::BadRequest("No geometry column found".to_string()))
    }

    /// Detect SRID of geometry column. Returns 4326 as default if unknown.
    fn detect_srid(&self, conn: &Connection, table_name: &str, geom_col: &str) -> i32 {
        let geom_qi = match quote_ident(geom_col) {
            Ok(q) => q,
            Err(_) => return 4326,
        };
        // Try ST_SRID first
        let sql = format!(
            "SELECT ST_SRID({}) FROM {} WHERE {} IS NOT NULL LIMIT 1",
            geom_qi, table_name, geom_qi
        );
        if let Ok(srid) = conn.query_row(&sql, [], |row| row.get::<_, i32>(0)) {
            if srid != 0 {
                tracing::info!("Detected SRID {} for {}.{}", srid, table_name, geom_col);
                return srid;
            }
        }

        // Heuristic: check coordinate ranges to detect projected CRS
        let bounds_sql = format!(
            "SELECT ST_XMin(ST_Extent({})), ST_XMax(ST_Extent({})), ST_YMin(ST_Extent({})), ST_YMax(ST_Extent({})) FROM {}",
            geom_qi, geom_qi, geom_qi, geom_qi, table_name
        );
        if let Ok((xmin, xmax, ymin, ymax)) = conn.query_row(&bounds_sql, [], |row| {
            Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?, row.get::<_, f64>(2)?, row.get::<_, f64>(3)?))
        }) {
            // If coordinates are clearly outside WGS84 range, it's projected
            if xmin.abs() > 360.0 || xmax.abs() > 360.0 || ymin.abs() > 90.0 || ymax.abs() > 90.0 {
                tracing::warn!(
                    "Coordinates out of WGS84 range ({}, {}, {}, {}) — likely projected CRS. Assuming EPSG:4326 anyway (reprojection may fail).",
                    xmin, ymin, xmax, ymax
                );
                // Can't reliably guess the exact projected CRS without metadata
                // Return 0 to signal "unknown projected"
                return 0;
            }
        }

        4326
    }

    fn detect_geometry_type(&self, conn: &Connection, table_name: &str, geom_col: &str) -> AppResult<GeometryType> {
        validate_identifier(table_name)?;
        let geom_qi = quote_ident(geom_col)?;
        let sql = format!(
            "SELECT ST_GeometryType({}) FROM {} WHERE {} IS NOT NULL LIMIT 1",
            geom_qi, table_name, geom_qi
        );
        
        let geom_type: String = conn.query_row(&sql, [], |row| row.get(0))
            .unwrap_or_else(|_| "UNKNOWN".to_string());

        let normalized = geom_type.to_uppercase().replace(" ", "");
        tracing::debug!("ST_GeometryType returned: {}", geom_type);
        
        if normalized == "UNKNOWN" || normalized.is_empty() {
            let geojson_sql = format!(
                "SELECT ST_AsGeoJSON({}) FROM {} WHERE {} IS NOT NULL LIMIT 1",
                geom_qi, table_name, geom_qi
            );
            
            if let Ok(geojson) = conn.query_row(&geojson_sql, [], |row| row.get::<_, String>(0)) {
                if let Some(start) = geojson.find("\"type\":\"") {
                    let rest = &geojson[start + 8..];
                    if let Some(end) = rest.find('"') {
                        let extracted = &rest[..end];
                        tracing::debug!("Extracted geometry type from GeoJSON: {}", extracted);
                        return Ok(GeometryType::from(extracted.to_uppercase().as_str()));
                    }
                }
            }
        }

        Ok(GeometryType::from(normalized.as_str()))
    }

    /// Internal bbox column names used for spatial indexing — excluded from user-facing metadata
    const INTERNAL_COLUMNS: &'static [&'static str] = &["_bbox_xmin", "_bbox_ymin", "_bbox_xmax", "_bbox_ymax"];

    fn get_column_info(&self, conn: &Connection, table_name: &str) -> AppResult<Vec<ColumnInfo>> {
        validate_identifier(table_name)?;
        let mut stmt = conn.prepare(
            "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_name = ?",
        )?;
        let columns = stmt
            .query_map(duckdb::params![table_name], |row| {
                Ok(ColumnInfo {
                    name: row.get(0)?,
                    data_type: row.get(1)?,
                    nullable: row.get::<_, String>(2)? == "YES",
                })
            })?
            .filter_map(|r| r.ok())
            .filter(|c| !Self::INTERNAL_COLUMNS.contains(&c.name.as_str()))
            .collect();

        Ok(columns)
    }

    fn get_bounds(&self, conn: &Connection, table_name: &str, geom_col: &str) -> AppResult<Bounds> {
        validate_identifier(table_name)?;
        let geom_qi = quote_ident(geom_col)?;
        // Use MIN/MAX on per-row envelope coordinates instead of ST_Extent,
        // which has issues returning only the first row's extent in some DuckDB modes.
        let sql = format!(
            r#"SELECT 
                MIN(ST_XMin(ST_Envelope({gc}))) as min_x,
                MIN(ST_YMin(ST_Envelope({gc}))) as min_y,
                MAX(ST_XMax(ST_Envelope({gc}))) as max_x,
                MAX(ST_YMax(ST_Envelope({gc}))) as max_y
            FROM {tbl}
            WHERE {gc} IS NOT NULL"#,
            gc = geom_qi, tbl = table_name
        );

        let bounds = conn.query_row(&sql, [], |row| {
            Ok(Bounds {
                min_x: row.get(0)?,
                min_y: row.get(1)?,
                max_x: row.get(2)?,
                max_y: row.get(3)?,
            })
        })?;

        Ok(bounds)
    }

    /// Get features as Arrow IPC stream.
    /// Streams batches directly to the IPC writer — never collects all batches in memory.
    pub fn get_features_arrow(&self, dataset_id: Uuid, limit: Option<i64>, offset: Option<i64>, bbox: Option<(f64, f64, f64, f64)>) -> AppResult<Vec<u8>> {
        let datasets = self.datasets.read().map_err(lock_err)?;
        let dataset = datasets.get(&dataset_id)
            .ok_or_else(|| AppError::NotFound(format!("Dataset {} not found", dataset_id)))?;

        validate_identifier(&dataset.table_name)?;
        let geom_qi = quote_ident(&dataset.geometry_column)?;

        let conn = self.read_conn()?;

        let non_geom_cols: Vec<String> = dataset.columns.iter()
            .filter(|c| c.name != dataset.geometry_column)
            .map(|c| quote_ident(&c.name))
            .collect::<AppResult<Vec<_>>>()?;
        
        let cols_str = if non_geom_cols.is_empty() {
            String::new()
        } else {
            non_geom_cols.join(", ") + ", "
        };

        // Two-stage spatial filter: fast bbox range scan on materialized columns,
        // then precise ST_Intersects refinement on candidates only.
        let bbox_clause = if let Some((minx, miny, maxx, maxy)) = bbox {
            validate_bbox(minx, miny, maxx, maxy)?;
            format!(
                " WHERE _bbox_xmax >= {} AND _bbox_xmin <= {} \
                 AND _bbox_ymax >= {} AND _bbox_ymin <= {} \
                 AND ST_Intersects({}, ST_MakeEnvelope({}, {}, {}, {}))",
                minx, maxx, miny, maxy,
                geom_qi, minx, miny, maxx, maxy
            )
        } else {
            String::new()
        };

        const CHUNK_SIZE: i64 = 5000;
        let total_limit = limit.unwrap_or(dataset.feature_count);
        let start_offset = offset.unwrap_or(0);
        
        // Pre-size buffer estimate: ~200 bytes per feature is a reasonable heuristic
        let estimated_size = (total_limit as usize).min(100_000) * 200;
        let mut ipc_buffer = Vec::with_capacity(estimated_size);
        let mut writer: Option<StreamWriter<&mut Vec<u8>>> = None;
        let mut current_offset = start_offset;
        let mut remaining = total_limit;
        let mut total_rows: usize = 0;

        while remaining > 0 {
            let chunk_limit = std::cmp::min(remaining, CHUNK_SIZE);
            
            let sql = format!(
                "SELECT {}ST_AsWKB({}) as geometry FROM {}{} LIMIT {} OFFSET {}",
                cols_str, geom_qi, dataset.table_name, bbox_clause, chunk_limit, current_offset
            );
            
            tracing::debug!("Arrow SQL: {}", sql);

            let mut stmt = conn.prepare(&sql).map_err(|e| {
                tracing::error!("Arrow prepare error: {}", e);
                e
            })?;
            let arrow_result = stmt.query_arrow([]).map_err(|e| {
                tracing::error!("Arrow query error: {}", e);
                e
            })?;
            
            let mut chunk_rows: usize = 0;
            for batch in arrow_result {
                if batch.num_rows() == 0 {
                    continue;
                }
                // Lazily initialize writer on first batch (captures schema)
                let w = match writer.as_mut() {
                    Some(w) => w,
                    None => {
                        let schema = batch.schema();
                        writer = Some(StreamWriter::try_new(&mut ipc_buffer, &schema)
                            .map_err(|e| AppError::Internal(format!("Arrow IPC writer error: {}", e)))?);
                        writer.as_mut().unwrap()
                    }
                };
                chunk_rows += batch.num_rows();
                w.write(&batch)
                    .map_err(|e| AppError::Internal(format!("Arrow IPC write error: {}", e)))?;
            }
            
            if chunk_rows == 0 {
                break;
            }

            total_rows += chunk_rows;
            current_offset += chunk_rows as i64;
            remaining -= chunk_rows as i64;
            
            if chunk_rows < CHUNK_SIZE as usize {
                break;
            }
        }
        
        if let Some(w) = writer.as_mut() {
            w.finish()
                .map_err(|e| AppError::Internal(format!("Arrow IPC finish error: {}", e)))?;
        } else {
            return Ok(Vec::new());
        }

        tracing::debug!("Arrow IPC: {} total rows, {} bytes for dataset {}", total_rows, ipc_buffer.len(), dataset_id);
        Ok(ipc_buffer)
    }

    /// Get features as GeoJSON with all property columns included
    pub fn get_features_geojson(&self, dataset_id: Uuid, limit: Option<i64>, offset: Option<i64>, bbox: Option<(f64, f64, f64, f64)>) -> AppResult<String> {
        let datasets = self.datasets.read().map_err(lock_err)?;
        let dataset = datasets.get(&dataset_id)
            .ok_or_else(|| AppError::NotFound(format!("Dataset {} not found", dataset_id)))?;

        validate_identifier(&dataset.table_name)?;
        let geom_qi = quote_ident(&dataset.geometry_column)?;

        let conn = self.read_conn()?;

        // Build property column list (all non-geometry columns)
        let prop_cols: Vec<(&ColumnInfo, String)> = dataset.columns.iter()
            .filter(|c| c.name != dataset.geometry_column)
            .map(|c| {
                let qi = quote_ident(&c.name)?;
                Ok((c, qi))
            })
            .collect::<AppResult<Vec<_>>>()?;

        // Build SELECT: geometry + all property columns
        let prop_select = if prop_cols.is_empty() {
            String::new()
        } else {
            ", ".to_string() + &prop_cols.iter().map(|(_, qi)| qi.as_str()).collect::<Vec<_>>().join(", ")
        };

        let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();
        let offset_clause = offset.map(|o| format!(" OFFSET {}", o)).unwrap_or_default();

        // Two-stage spatial filter: fast bbox range scan + precise ST_Intersects
        let bbox_clause = if let Some((minx, miny, maxx, maxy)) = bbox {
            validate_bbox(minx, miny, maxx, maxy)?;
            format!(
                " WHERE _bbox_xmax >= {} AND _bbox_xmin <= {} \
                 AND _bbox_ymax >= {} AND _bbox_ymin <= {} \
                 AND ST_Intersects({}, ST_MakeEnvelope({}, {}, {}, {}))",
                minx, maxx, miny, maxy,
                geom_qi, minx, miny, maxx, maxy
            )
        } else {
            String::new()
        };

        let sql = format!(
            "SELECT ST_AsGeoJSON({}){} FROM {}{}{}{}",
            geom_qi, prop_select, dataset.table_name, bbox_clause, limit_clause, offset_clause
        );

        let num_props = prop_cols.len();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let geom: String = row.get(0)?;
            let mut props = Vec::with_capacity(num_props);
            for i in 0..num_props {
                // Read raw value and convert to string — DuckDB won't auto-cast numeric types
                use duckdb::types::Value;
                let val: Value = row.get(i + 1)?;
                let str_val = match val {
                    Value::Null => None,
                    Value::Boolean(b) => Some(b.to_string()),
                    Value::TinyInt(n) => Some(n.to_string()),
                    Value::SmallInt(n) => Some(n.to_string()),
                    Value::Int(n) => Some(n.to_string()),
                    Value::BigInt(n) => Some(n.to_string()),
                    Value::HugeInt(n) => Some(n.to_string()),
                    Value::UTinyInt(n) => Some(n.to_string()),
                    Value::USmallInt(n) => Some(n.to_string()),
                    Value::UInt(n) => Some(n.to_string()),
                    Value::UBigInt(n) => Some(n.to_string()),
                    Value::Float(n) => Some(n.to_string()),
                    Value::Double(n) => Some(n.to_string()),
                    Value::Text(s) => Some(s),
                    _ => Some(format!("{:?}", val)),
                };
                props.push(str_val);
            }
            Ok((geom, props))
        })?;

        let mut features = Vec::new();
        for (idx, row_result) in rows.enumerate() {
            let (geom, props) = match row_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Skipping GeoJSON row {}: {}", idx, e);
                    continue;
                }
            };
            // Build properties JSON object
            let props_json = if prop_cols.is_empty() {
                "{}".to_string()
            } else {
                let pairs: Vec<String> = prop_cols.iter()
                    .zip(props.iter())
                    .map(|((col, _), val)| {
                        match val {
                            Some(v) => {
                                // Try to preserve numeric types
                                if v.parse::<f64>().is_ok() && !v.contains(|c: char| c.is_alphabetic()) {
                                    format!("\"{}\":{}", col.name, v)
                                } else {
                                    // Escape string value for JSON
                                    let escaped = v.replace('\\', "\\\\").replace('"', "\\\"");
                                    format!("\"{}\":\"{}\"", col.name, escaped)
                                }
                            }
                            None => format!("\"{}\":null", col.name),
                        }
                    })
                    .collect();
                format!("{{{}}}", pairs.join(","))
            };

            features.push(format!(
                r#"{{"type":"Feature","id":{},"geometry":{},"properties":{}}}"#,
                idx, geom, props_json
            ));
        }

        let geojson = format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        );

        Ok(geojson)
    }

    /// Get dataset bounds
    pub fn get_dataset_bounds(&self, dataset_id: Uuid) -> AppResult<Bounds> {
        let datasets = self.datasets.read().map_err(lock_err)?;
        let dataset = datasets.get(&dataset_id)
            .ok_or_else(|| AppError::NotFound(format!("Dataset {} not found", dataset_id)))?;

        dataset.bounds.clone()
            .ok_or_else(|| AppError::Internal("Bounds not available".to_string()))
    }

    /// List all datasets
    pub fn list_datasets(&self) -> AppResult<Vec<Dataset>> {
        let datasets = self.datasets.read().map_err(lock_err)?;
        Ok(datasets.values().cloned().collect())
    }

    /// Get a single dataset
    pub fn get_dataset(&self, id: Uuid) -> AppResult<Dataset> {
        let datasets = self.datasets.read().map_err(lock_err)?;
        datasets.get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Dataset {} not found", id)))
    }

    /// Delete a dataset
    pub fn delete_dataset(&self, id: Uuid) -> AppResult<()> {
        let mut datasets = self.datasets.write().map_err(lock_err)?;
        let dataset = datasets.remove(&id)
            .ok_or_else(|| AppError::NotFound(format!("Dataset {} not found", id)))?;

        validate_identifier(&dataset.table_name)?;
        let conn = self.write_conn()?;
        conn.execute(&format!("DROP TABLE IF EXISTS {}", dataset.table_name), [])?;
        conn.execute("DELETE FROM _meta_datasets WHERE id = ?", duckdb::params![id.to_string()])?;

        Ok(())
    }

    // ========================================================================
    // Map Configuration Methods
    // ========================================================================

    pub fn create_map(&self, name: String, description: Option<String>, basemap: Option<String>, view: Option<MapView>) -> AppResult<MapConfig> {
        let now = Utc::now();
        let map = MapConfig {
            id: Uuid::new_v4(),
            name,
            description,
            basemap: basemap.unwrap_or_else(|| "osm".to_string()),
            view: view.unwrap_or_default(),
            layers: Vec::new(),
            created_at: now,
            updated_at: now,
        };

        let conn = self.write_conn()?;
        self.persist_map_meta(&conn, &map)?;
        drop(conn);

        self.maps.write().map_err(lock_err)?
            .insert(map.id, map.clone());

        Ok(map)
    }

    pub fn list_maps(&self) -> AppResult<Vec<MapConfig>> {
        let maps = self.maps.read().map_err(lock_err)?;
        Ok(maps.values().cloned().collect())
    }

    pub fn get_map(&self, id: Uuid) -> AppResult<MapConfig> {
        let maps = self.maps.read().map_err(lock_err)?;
        maps.get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Map {} not found", id)))
    }

    pub fn update_map(&self, id: Uuid, name: Option<String>, description: Option<String>, basemap: Option<String>, view: Option<MapView>, layers: Option<Vec<crate::models::MapLayer>>) -> AppResult<MapConfig> {
        let mut maps = self.maps.write().map_err(lock_err)?;
        let map = maps.get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Map {} not found", id)))?;

        if let Some(n) = name { map.name = n; }
        if let Some(d) = description { map.description = Some(d); }
        if let Some(b) = basemap { map.basemap = b; }
        if let Some(v) = view { map.view = v; }
        if let Some(l) = layers { map.layers = l; }
        map.updated_at = Utc::now();

        let updated = map.clone();
        drop(maps);

        let conn = self.write_conn()?;
        self.persist_map_meta(&conn, &updated)?;

        Ok(updated)
    }

    pub fn delete_map(&self, id: Uuid) -> AppResult<()> {
        let mut maps = self.maps.write().map_err(lock_err)?;
        maps.remove(&id)
            .ok_or_else(|| AppError::NotFound(format!("Map {} not found", id)))?;
        drop(maps);

        let conn = self.write_conn()?;
        conn.execute("DELETE FROM _meta_maps WHERE id = ?", duckdb::params![id.to_string()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Create an in-memory DuckDbManager with a unique temp data directory per test.
    fn make_db() -> DuckDbManager {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("geoconsole_test_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        DuckDbManager::new_in_memory(dir).expect("Failed to init DuckDB")
    }

    /// Write test geojson into the db's data directory and load it.
    fn load_test_geojson(db: &DuckDbManager) -> Dataset {
        let path = db.db_path.join("test.geojson");
        std::fs::write(&path, r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[151.2,-33.8]},"properties":{"name":"Sydney","pop":5000000}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[144.9,-37.8]},"properties":{"name":"Melbourne","pop":4900000}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[153.0,-27.5]},"properties":{"name":"Brisbane","pop":2500000}}
            ]
        }"#).unwrap();
        db.load_spatial_file(path.to_str().unwrap(), "test_cities").unwrap()
    }

    #[test]
    fn test_init() {
        let _db = make_db();
    }

    #[test]
    fn test_load_geojson() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        assert_eq!(ds.feature_count, 3);
        assert_eq!(ds.name, "test_cities");
    }

    #[test]
    fn test_detect_geometry_column() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        assert!(!ds.geometry_column.is_empty());
    }

    #[test]
    fn test_detect_geometry_type() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        let gt = format!("{:?}", ds.geometry_type).to_uppercase();
        assert!(gt.contains("POINT"), "Expected Point, got {:?}", ds.geometry_type);
    }

    #[test]
    fn test_get_bounds() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        let bounds = ds.bounds.unwrap();
        assert!(bounds.min_x <= bounds.max_x);
        assert!(bounds.min_y <= bounds.max_y);
        assert!(bounds.min_x < 145.0, "min_x={} should be < 145.0", bounds.min_x);
        assert!(bounds.max_x > 151.0, "max_x={} should be > 151.0", bounds.max_x);
    }

    #[test]
    fn test_arrow_ipc_generation() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        let bytes = db.get_features_arrow(ds.id, None, None, None).unwrap();
        assert!(!bytes.is_empty(), "Arrow IPC should not be empty");
        assert!(bytes.len() > 100);
    }

    #[test]
    fn test_arrow_ipc_with_bbox() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        let bbox = Some((150.0, -34.5, 152.0, -33.0));
        let bytes = db.get_features_arrow(ds.id, None, None, bbox).unwrap();
        assert!(!bytes.is_empty());
        let bbox_empty = Some((0.0, -0.5, 1.0, 1.0));
        let bytes_empty = db.get_features_arrow(ds.id, None, None, bbox_empty).unwrap();
        assert!(bytes_empty.len() < bytes.len());
    }

    #[test]
    fn test_geojson_with_bbox() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        let bbox = Some((150.0, -34.5, 152.0, -33.0));
        let geojson = db.get_features_geojson(ds.id, None, None, bbox).unwrap();
        assert!(geojson.contains("Sydney"));
        assert!(!geojson.contains("Melbourne"));
    }

    #[test]
    fn test_geojson_includes_properties() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        let geojson = db.get_features_geojson(ds.id, None, None, None).unwrap();
        assert!(geojson.contains("\"name\""), "GeoJSON should include property columns");
        assert!(geojson.contains("Sydney"), "GeoJSON should include property values");
        assert!(geojson.contains("5000000"), "GeoJSON should include numeric properties");
    }

    #[test]
    fn test_dataset_crud() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        
        let list = db.list_datasets().unwrap();
        assert_eq!(list.len(), 1);
        
        let got = db.get_dataset(ds.id).unwrap();
        assert_eq!(got.name, "test_cities");
        
        db.delete_dataset(ds.id).unwrap();
        let list2 = db.list_datasets().unwrap();
        assert_eq!(list2.len(), 0);
        
        assert!(db.get_dataset(ds.id).is_err());
    }

    #[test]
    fn test_bbox_columns_excluded_from_metadata() {
        let db = make_db();
        let ds = load_test_geojson(&db);
        let col_names: Vec<&str> = ds.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(!col_names.contains(&"_bbox_xmin"), "Internal bbox columns should be hidden");
        assert!(!col_names.contains(&"_bbox_ymin"), "Internal bbox columns should be hidden");
        assert!(!col_names.contains(&"_bbox_xmax"), "Internal bbox columns should be hidden");
        assert!(!col_names.contains(&"_bbox_ymax"), "Internal bbox columns should be hidden");
    }
}
