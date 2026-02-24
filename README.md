# GeoConsole V3 - Map Studio

A modern geospatial map authoring platform with a Rust backend, DuckDB spatial database, Arrow IPC data transfer, and Svelte 5 frontend.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         GeoConsole V3                                │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐ │
│  │   Svelte 5 UI   │◄──►│  Rust Backend   │◄──►│    DuckDB       │ │
│  │   (Map Studio)  │    │  (Axum + Arrow) │    │  (Spatial)      │ │
│  └────────┬────────┘    └────────┬────────┘    └────────┬────────┘ │
│           │                      │                      │          │
│           │              Arrow IPC                      │          │
│           │              (binary)           GeoParquet  │          │
│           │                      │              load    │          │
│           ▼                      ▼                      ▼          │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐ │
│  │  MapLibre GL    │    │    Valhalla     │    │  User Files     │ │
│  │  (Rendering)    │    │    (Docker)     │    │  (.parquet)     │ │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

## Tech Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Backend** | Rust + Axum | HTTP API server |
| **Database** | DuckDB + Spatial | Embedded spatial database |
| **Data Format** | Arrow IPC | Zero-copy binary data transfer |
| **Ingestion** | GeoParquet | User file uploads |
| **Routing** | Valhalla (Docker) | OSM-based routing engine |
| **Frontend** | Svelte 5 + Vite | Map Studio UI |
| **Map Engine** | MapLibre GL JS | Vector map rendering |

## Quick Start

### Prerequisites

- Rust 1.75+
- Node.js 20+
- Docker & Docker Compose

### 1. Start Valhalla Routing Service

```bash
docker-compose up -d
```

This downloads OSM data for Australia and builds routing tiles (first run takes ~30 minutes).

### 2. Start Rust Backend

```bash
cargo run
```

The API server starts at `http://localhost:3000`.

### 3. Start Frontend Development Server

```bash
cd frontend
npm install
npm run dev
```

The frontend is available at `http://localhost:5173`.

## API Endpoints

### Health Check
- `GET /health` - Service health status

### Datasets
- `GET /api/datasets` - List all datasets
- `POST /api/datasets` - Upload GeoParquet file
- `GET /api/datasets/:id` - Get dataset metadata
- `DELETE /api/datasets/:id` - Delete dataset

### Spatial Data
- `GET /api/datasets/:id/features` - Get features as Arrow IPC
- `GET /api/datasets/:id/geojson` - Get features as GeoJSON
- `GET /api/datasets/:id/bounds` - Get dataset bounds

### Map Configuration
- `GET /api/maps` - List all map configurations
- `POST /api/maps` - Create new map
- `GET /api/maps/:id` - Get map configuration
- `POST /api/maps/:id` - Update map configuration
- `DELETE /api/maps/:id` - Delete map

### Routing (Valhalla Proxy)
- `POST /api/routing/route` - Calculate route
- `POST /api/routing/isochrone` - Calculate isochrone

## Data Workflow

1. **Upload** - User uploads a spatial file (GeoParquet, GeoPackage, Shapefile, GeoJSON, KML)
2. **Server DuckDB** - Backend loads into DuckDB with spatial extension (source of truth)
3. **Arrow IPC Transfer** - Server streams Arrow IPC binary to browser
4. **Browser DuckDB WASM** - Cached locally with TTL for fast re-queries (no server round-trip)
5. **SQL Filter** - User WHERE clause runs against browser DuckDB WASM cache
6. **deck.gl Render** - Arrow Table → GeoArrow layers (zero-copy GPU rendering)

## Project Structure

```
D:\_GeoConsole_V3/
├── Cargo.toml              # Rust dependencies
├── docker-compose.yml      # Valhalla service
├── src/
│   ├── main.rs            # Axum server entry point
│   ├── api/               # API route handlers
│   │   ├── datasets.rs    # Dataset CRUD
│   │   ├── maps.rs        # Map configuration
│   │   ├── routing.rs     # Valhalla proxy
│   │   └── spatial.rs     # Arrow IPC / GeoJSON
│   ├── db.rs              # DuckDB manager
│   ├── error.rs           # Error handling
│   └── models.rs          # Data models
├── frontend/
│   ├── package.json       # Node dependencies
│   ├── src/
│   │   ├── App.svelte     # Main app
│   │   └── lib/
│   │       ├── components/
│   │       │   ├── MapStudio.svelte
│   │       │   ├── LayerPanel.svelte
│   │       │   ├── StyleEditor.svelte
│   │       │   ├── SqlFilter.svelte
│   │       │   └── BasemapSelector.svelte
│   │       ├── stores/
│   │       │   └── mapStudio.svelte.ts
│   │       ├── services/
│   │       │   ├── api.ts
│   │       │   ├── duckdb.ts
│   │       │   └── deckgl.ts
│   │       └── types/
│   │           └── mapStudio.ts
└── .windsurf/
    └── rules/             # Workspace rules
```

## Features

### Map Studio
- **Layer Management** - Add, remove, reorder, toggle visibility
- **Styling Controls** - Fill/stroke colors, opacity, width, point radius
- **Field-Based Symbolisation** - Color features by attribute field (graduated for numeric, categorized for text)
- **SQL Filter** - Filter displayed features with SQL WHERE clauses (runs in browser DuckDB WASM)
- **Basemap Selection** - OSM, CartoDB, Satellite, Terrain, None
- **Color Ramps** - 8 preset color ramps (Blues, Greens, Reds, Viridis, Spectral, etc.)
- **Export/Import** - Save map configuration as JSON

### Data Support
- **Multi-Format Ingestion** - GeoParquet, GeoPackage, Shapefile, GeoJSON, KML
- **Arrow IPC** - High-performance binary data transfer (streamed, constant memory)
- **Browser DuckDB WASM** - Local cache with TTL, OPFS persistence, IndexedDB fallback
- **deck.gl GeoArrow** - Zero-copy Arrow → GPU rendering
- **GeoJSON** - Standard GeoJSON output

### Routing (Valhalla)
- **Route Calculation** - Auto, bicycle, pedestrian modes
- **Isochrones** - Travel time polygons

## Development

### Build for Production

```bash
# Backend
cargo build --release

# Frontend
cd frontend
npm run build
```

### Run Tests

```bash
cargo test
```

## License

MIT
