import type { MapStudioState, MapLayer, MapView, LayerStyle, Dataset, GeometryType } from '../types/mapStudio';
import { DEFAULT_STYLES, BASEMAPS } from '../types/mapStudio';

function createMapStudioStore() {
  let state = $state<MapStudioState>({
    id: crypto.randomUUID(),
    name: 'Untitled Map',
    description: '',
    basemap: 'osm',
    view: {
      center: [133.7751, -25.2744], // Australia
      zoom: 4,
      bearing: 0,
      pitch: 0
    },
    layers: [],
    selectedLayerId: null,
    showLayerPanel: true,
    showStyleEditor: true,
    createdAt: new Date(),
    updatedAt: new Date()
  });

  // Datasets from backend
  let datasets = $state<Dataset[]>([]);

  return {
    get state() { return state; },
    get datasets() { return datasets; },
    get selectedLayer() {
      return state.layers.find(l => l.id === state.selectedLayerId) ?? null;
    },
    get currentBasemap() {
      return BASEMAPS.find(b => b.id === state.basemap) ?? BASEMAPS[0];
    },

    // Layer management
    addLayer(layer: Omit<MapLayer, 'id' | 'zIndex'>) {
      const newLayer: MapLayer = {
        ...layer,
        id: crypto.randomUUID(),
        zIndex: state.layers.length
      };
      state.layers = [...state.layers, newLayer];
      state.selectedLayerId = newLayer.id;
      state.updatedAt = new Date();
      return newLayer;
    },

    addLayerFromDataset(dataset: Dataset) {
      const rawGeomType = dataset.geometry_type || 'polygon';
      // Normalize to PascalCase for style lookup
      const geomTypeMap: Record<string, GeometryType> = {
        'point': 'Point', 'linestring': 'LineString', 'polygon': 'Polygon',
        'multipoint': 'MultiPoint', 'multilinestring': 'MultiLineString', 'multipolygon': 'MultiPolygon'
      };
      const geometryType = geomTypeMap[rawGeomType.toLowerCase()] || 'Polygon';
      const defaultStyle = DEFAULT_STYLES[geometryType] || DEFAULT_STYLES['Polygon'];
      
      return this.addLayer({
        name: dataset.name,
        type: 'geojson',
        visible: true,
        opacity: 1,
        datasetId: dataset.id,
        geometryType: geometryType,
        geometryColumn: dataset.geometry_column,
        style: { ...defaultStyle },
        featureCount: dataset.feature_count,
        bounds: dataset.bounds ? [
          dataset.bounds.min_x,
          dataset.bounds.min_y,
          dataset.bounds.max_x,
          dataset.bounds.max_y
        ] : undefined
      });
    },

    removeLayer(layerId: string) {
      state.layers = state.layers.filter(l => l.id !== layerId);
      if (state.selectedLayerId === layerId) {
        state.selectedLayerId = state.layers[0]?.id ?? null;
      }
      // Reindex z-indices
      state.layers = state.layers.map((l, i) => ({ ...l, zIndex: i }));
      state.updatedAt = new Date();
    },

    selectLayer(layerId: string | null) {
      state.selectedLayerId = layerId;
    },

    updateLayer(layerId: string, updates: Partial<MapLayer>) {
      state.layers = state.layers.map(l =>
        l.id === layerId ? { ...l, ...updates } : l
      );
      state.updatedAt = new Date();
    },

    updateLayerStyle(layerId: string, style: Partial<LayerStyle>) {
      state.layers = state.layers.map(l =>
        l.id === layerId ? { ...l, style: { ...l.style, ...style } } : l
      );
      state.updatedAt = new Date();
    },

    setLayerVisibility(layerId: string, visible: boolean) {
      this.updateLayer(layerId, { visible });
    },

    setLayerOpacity(layerId: string, opacity: number) {
      this.updateLayer(layerId, { opacity });
    },

    moveLayer(layerId: string, direction: 'up' | 'down') {
      const idx = state.layers.findIndex(l => l.id === layerId);
      if (idx === -1) return;
      
      const newIdx = direction === 'up' ? idx + 1 : idx - 1;
      if (newIdx < 0 || newIdx >= state.layers.length) return;

      const newLayers = [...state.layers];
      [newLayers[idx], newLayers[newIdx]] = [newLayers[newIdx], newLayers[idx]];
      state.layers = newLayers.map((l, i) => ({ ...l, zIndex: i }));
      state.updatedAt = new Date();
    },

    setLayerData(layerId: string, data: GeoJSON.FeatureCollection) {
      this.updateLayer(layerId, { data, featureCount: data.features.length });
    },

    // Basemap
    setBasemap(basemapId: string) {
      state.basemap = basemapId;
      state.updatedAt = new Date();
    },

    // View
    setView(view: Partial<MapView>) {
      state.view = { ...state.view, ...view };
    },

    // UI state
    toggleLayerPanel() {
      state.showLayerPanel = !state.showLayerPanel;
    },

    toggleStyleEditor() {
      state.showStyleEditor = !state.showStyleEditor;
    },

    // Map name
    setMapName(name: string) {
      state.name = name;
      state.updatedAt = new Date();
    },

    // Datasets
    setDatasets(newDatasets: Dataset[]) {
      datasets = newDatasets;
    },

    // Export
    exportState(): MapStudioState {
      return { ...state };
    },

    // Reset
    reset() {
      state = {
        id: crypto.randomUUID(),
        name: 'Untitled Map',
        description: '',
        basemap: 'osm',
        view: {
          center: [133.7751, -25.2744],
          zoom: 4,
          bearing: 0,
          pitch: 0
        },
        layers: [],
        selectedLayerId: null,
        showLayerPanel: true,
        showStyleEditor: true,
        createdAt: new Date(),
        updatedAt: new Date()
      };
    }
  };
}

export const mapStudioStore = createMapStudioStore();
