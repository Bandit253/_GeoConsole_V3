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

1. **Upload GeoParquet** - User uploads a `.parquet` or `.geoparquet` file
2. **Load into DuckDB** - Backend creates a table with spatial extension
3. **Query via Arrow IPC** - Frontend requests features in Arrow IPC format
4. **Render on Map** - MapLibre GL displays the data

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
│   │       │   └── BasemapSelector.svelte
│   │       ├── stores/
│   │       │   └── mapStudio.svelte.ts
│   │       ├── services/
│   │       │   └── api.ts
│   │       └── types/
│   │           └── mapStudio.ts
└── .windsurf/
    └── rules/             # Workspace rules
```

## Features

### Map Studio
- **Layer Management** - Add, remove, reorder, toggle visibility
- **Styling Controls** - Fill/stroke colors, opacity, width, point radius
- **Basemap Selection** - OSM, CartoDB, Satellite, Terrain, None
- **Color Ramps** - 8 preset color ramps for graduated styling
- **Export/Import** - Save map configuration as JSON

### Data Support
- **GeoParquet** - Load spatial data from Parquet files
- **Arrow IPC** - High-performance binary data transfer
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
