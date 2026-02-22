use geoconsole_v3::db::DuckDbManager;
use std::sync::atomic::{AtomicU32, Ordering};

static INTEG_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Create an in-memory DuckDbManager with a unique temp data directory per test.
fn make_db() -> DuckDbManager {
    let id = INTEG_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir()
        .join(format!("geoconsole_integ_{}_{}", std::process::id(), id));
    let _ = std::fs::remove_dir_all(&dir);
    DuckDbManager::new_in_memory(dir).expect("Failed to init DuckDB with spatial extension")
}

/// Write test GeoJSON into the db's data directory and return the path.
fn write_test_geojson(db: &DuckDbManager) -> String {
    let path = db.db_path.join("cities.geojson");
    std::fs::write(&path, r#"{
        "type": "FeatureCollection",
        "features": [
            {"type":"Feature","geometry":{"type":"Point","coordinates":[151.2093,-33.8688]},"properties":{"name":"Sydney","population":5312000}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[144.9631,-37.8136]},"properties":{"name":"Melbourne","population":4936000}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[153.0251,-27.4698]},"properties":{"name":"Brisbane","population":2514000}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[115.8605,-31.9505]},"properties":{"name":"Perth","population":2085000}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[138.6007,-34.9285]},"properties":{"name":"Adelaide","population":1345000}}
        ]
    }"#).unwrap();
    path.to_string_lossy().to_string()
}

#[test]
fn test_duckdb_init_and_spatial() {
    let _db = make_db();
    // If we get here, DuckDB initialized and spatial extension loaded successfully
}

#[test]
fn test_load_geojson_dataset() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "cities").unwrap();
    assert_eq!(ds.feature_count, 5);
    assert_eq!(ds.name, "cities");
    assert!(!ds.geometry_column.is_empty());
}

#[test]
fn test_geometry_type_detection() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "cities").unwrap();
    let gt = format!("{:?}", ds.geometry_type).to_uppercase();
    assert!(gt.contains("POINT"), "Expected Point type, got: {}", gt);
}

#[test]
fn test_bounds_calculation() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "cities").unwrap();
    let bounds = ds.bounds.unwrap();
    
    // Perth is westmost (~115.8), Brisbane is eastmost (~153.0)
    assert!(bounds.min_x < 116.0, "min_x should be near Perth: {}", bounds.min_x);
    assert!(bounds.max_x > 152.0, "max_x should be near Brisbane: {}", bounds.max_x);
    // Melbourne is southmost (~-37.8), Brisbane northmost (~-27.4)
    assert!(bounds.min_y < -37.0, "min_y should be near Melbourne: {}", bounds.min_y);
    assert!(bounds.max_y > -28.0, "max_y should be near Brisbane: {}", bounds.max_y);
}

#[test]
fn test_arrow_ipc_produces_valid_bytes() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "cities").unwrap();
    
    let bytes = db.get_features_arrow(ds.id, None, None, None).unwrap();
    assert!(!bytes.is_empty(), "Arrow IPC bytes should not be empty");
    assert!(bytes.len() > 50, "Arrow IPC should have substantial content");
}

#[test]
fn test_arrow_ipc_with_wkb_geometry() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "cities").unwrap();
    
    let bytes = db.get_features_arrow(ds.id, Some(2), None, None).unwrap();
    assert!(!bytes.is_empty());
    // WKB geometry produces binary data — the IPC should contain binary column
}

#[test]
fn test_bbox_filtering() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "cities").unwrap();
    
    // Bbox around Sydney only (roughly 150-152 lng, -34 to -33 lat)
    let bbox = Some((150.0, -34.5, 152.5, -33.0));
    let geojson = db.get_features_geojson(ds.id, None, None, bbox).unwrap();
    assert!(geojson.contains("Sydney"), "Should contain Sydney");
    assert!(!geojson.contains("Melbourne"), "Should NOT contain Melbourne");
    assert!(!geojson.contains("Perth"), "Should NOT contain Perth");
    
    // Bbox around nothing
    let bbox_empty = Some((0.0, 0.0, 1.0, 1.0));
    let geojson_empty = db.get_features_geojson(ds.id, None, None, bbox_empty).unwrap();
    assert!(!geojson_empty.contains("Sydney"));
}

#[test]
fn test_bbox_arrow_filtering() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "cities").unwrap();
    
    // Full dataset
    let all = db.get_features_arrow(ds.id, None, None, None).unwrap();
    
    // Bbox around Sydney only
    let bbox = Some((150.0, -34.5, 152.5, -33.0));
    let filtered = db.get_features_arrow(ds.id, None, None, bbox).unwrap();
    
    // Filtered should be smaller
    assert!(filtered.len() < all.len(), "Bbox-filtered Arrow should be smaller than full");
}

#[test]
fn test_dataset_crud() {
    let db = make_db();
    let path = write_test_geojson(&db);
    
    // Create
    let ds = db.load_spatial_file(&path, "crud_test").unwrap();
    let id = ds.id;
    
    // List
    let list = db.list_datasets().unwrap();
    assert!(list.iter().any(|d| d.id == id));
    
    // Get
    let got = db.get_dataset(id).unwrap();
    assert_eq!(got.name, "crud_test");
    
    // Delete
    db.delete_dataset(id).unwrap();
    
    // Verify deleted
    assert!(db.get_dataset(id).is_err());
    let list2 = db.list_datasets().unwrap();
    assert!(!list2.iter().any(|d| d.id == id));
}

#[test]
fn test_geojson_endpoint_still_works() {
    let db = make_db();
    let path = write_test_geojson(&db);
    let ds = db.load_spatial_file(&path, "geojson_test").unwrap();
    
    let geojson = db.get_features_geojson(ds.id, Some(3), None, None).unwrap();
    assert!(geojson.contains("FeatureCollection"));
    assert!(geojson.contains("Feature"));
    // Should have geometry content
    assert!(geojson.contains("coordinates") || geojson.contains("Point"));
}
