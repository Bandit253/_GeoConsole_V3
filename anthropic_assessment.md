Right, I've read the full codebase. Here's the assessment:

---

## GeoConsole V3 — Architecture Review

**Stack:** Rust (Axum) backend + DuckDB in-memory + Svelte 5 frontend with DuckDB-WASM + deck.gl + MapLibre

### 🔴 Critical Issues

**1. Single-threaded DuckDB behind a global `Mutex`**
The entire backend bottlenecks on `Arc<Mutex<Connection>>`. Every request — reads, writes, metadata lookups — contends for the same lock. DuckDB's Rust binding uses a single `Connection`, and `Mutex` means only one `spawn_blocking` task runs a query at a time. Under concurrent load, this serialises everything.

**Fix:** Use a connection pool pattern. DuckDB supports multiple read connections via `Connection::open` on the same file (switch from in-memory to file-based), or use `r2d2-duckdb`. Alternatively, use `RwLock` for read-heavy workloads so reads don't block each other.

**2. In-memory storage = total data loss on restart**
Datasets, maps, and all DuckDB tables live purely in RAM. Stop the process and everything's gone. The uploaded files persist in `data/` but the metadata (`HashMap<Uuid, Dataset>`, `HashMap<Uuid, MapConfig>`) doesn't.

**Fix:** Use a file-based DuckDB (`:memory:` → `geoconsole.duckdb`) and persist map configs in a DuckDB table or SQLite sidecar.

**3. SQL injection everywhere**
All queries use `format!()` string interpolation with user-influenced values (table names, column names, file paths). While table names are UUID-generated, the file path in `load_spatial_file` is constructed from user-uploaded filenames — a path traversal / SQL injection vector.

```rust
format!("CREATE TABLE {} AS SELECT * FROM ST_Read('{}')", table_name, file_path.replace("\\", "/"))
```

**Fix:** Sanitize/validate all interpolated values. For file paths, canonicalize and restrict to `data/` directory. For column names, whitelist against `information_schema` results.

### 🟡 Performance Issues

**4. GeoJSON serialization is wasteful**
`get_features_geojson` builds a `FeatureCollection` by string concatenation with no property data — every feature has `"properties":{}`. The frontend then has to re-parse this giant JSON string. Meanwhile, the Arrow IPC path exists but the frontend *still* converts Arrow → GeoJSON for MapLibre, negating the zero-copy benefit.

**Fix:** Either use deck.gl's native Arrow layer support (deck.gl v9 has this) to skip GeoJSON conversion entirely, or at minimum include properties in the GeoJSON endpoint.

**5. Chunked Arrow IPC collects all batches into memory**
`get_features_arrow` reads 5000-row chunks but accumulates *all* batches in `all_batches: Vec<RecordBatch>` before serializing. For a million-row dataset, this doubles memory usage (data + IPC buffer).

**Fix:** Stream batches directly to the IPC writer instead of collecting. Use Axum's streaming response (`Body::from_stream`) to write Arrow IPC incrementally.

**6. No spatial indexing**
Bbox queries use `ST_Intersects` with full table scans. DuckDB's spatial extension doesn't auto-index, so every bbox query is O(n).

**Fix:** Create R-tree indices or pre-compute Hilbert-sorted partitions. For the bbox endpoint, use DuckDB's `CREATE INDEX` with spatial extension if available, or maintain a separate bbox lookup table.

**7. Frontend double-parses everything**
The pipeline is: Server DuckDB → Arrow IPC → Frontend `tableFromIPC()` → parse WKB row-by-row → GeoJSON objects → deck.gl re-flattens Multi* geometries. This is a lot of per-feature JavaScript work.

**Fix:** Use deck.gl's `GeoArrowScatterplotLayer` / `GeoArrowPolygonLayer` from `@deck.gl/geo-layers` which consume Arrow tables with WKB/GeoArrow columns directly — no per-row parsing needed.

**8. `reqwest::Client` created per-request in routing**
Both `calculate_route` and `calculate_isochrone` create a new `reqwest::Client` on every call. This means new connection pools and TLS handshakes each time.

**Fix:** Put a shared `reqwest::Client` in `AppState` and reuse it.

### 🟡 Architectural Concerns

**9. Duplicate code in `db.rs`**
`load_geopackage`, `load_shapefile`, `load_geojson`, `load_kml` are all identical (`ST_Read`). `load_geoparquet_internal` is dead code. The `load_parquet` method is separate because it uses `read_parquet` instead of `ST_Read`, which is correct, but the rest should be one function.

**10. Browser DuckDB-WASM cache is ephemeral too**
The IndexedDB stores cache *metadata* but actual DuckDB-WASM tables are in-memory and lost on page refresh. The code handles this (`validateCachedTables`) but it means every refresh re-fetches everything from the server.

**Fix:** Use DuckDB-WASM's OPFS (Origin Private File System) persistence to survive refreshes, or cache the raw Arrow IPC buffers in IndexedDB directly.

**11. No pagination strategy for large datasets**
The frontend fetches entire datasets (up to `feature_count` rows). For datasets with 100K+ features, this will OOM the browser tab.

**Fix:** Implement viewport-based loading — only fetch features within the current map bbox. The bbox parameter exists but isn't used from the frontend map view.

### 🟢 Opportunities

**12. Vector tiles** — For large datasets, generate Mapbox Vector Tiles (MVT) server-side instead of shipping raw geometry. DuckDB + `ST_AsMVT` or a sidecar like Martin/pg_tileserv equivalent.

**13. Release build** — The `target/debug` directory suggests it's only been run in debug mode. Rust debug builds are 10-50x slower. `cargo build --release` is free performance.

**14. Connection pooling for Valhalla** — If routing becomes heavily used, consider connection keep-alive and request batching.

**15. SRID assumption** — Hardcoded `srid: 4326`. If someone uploads data in a projected CRS, bounds and bbox filtering will be wrong silently.

---

**TL;DR:** The biggest wins are: (1) fix the Mutex bottleneck, (2) persist to disk, (3) use deck.gl's native GeoArrow layers to skip the row-by-row WKB→GeoJSON conversion, and (4) implement viewport-based feature loading. The SQL injection risk is also worth addressing before this sees any network exposure.