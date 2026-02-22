// Map Studio Type Definitions

export type LayerType = 'vector' | 'raster' | 'deck.gl' | 'geojson';
export type GeometryType = 'Point' | 'LineString' | 'Polygon' | 'MultiPoint' | 'MultiLineString' | 'MultiPolygon';
export type StyleType = 'simple' | 'categorized' | 'graduated' | 'heatmap';

export interface Basemap {
  id: string;
  label: string;
  url: string;
  attribution: string;
  thumbnail?: string;
  maxzoom?: number;
}

export interface LabelConfig {
  enabled: boolean;
  field: string;
  fontSize: number;
  fontColor: string;
  fontWeight: 'normal' | 'bold';
  haloColor: string;
  haloWidth: number;
  offsetX: number;
  offsetY: number;
  anchor: 'center' | 'top' | 'bottom' | 'left' | 'right' | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';
  maxWidth: number;
}

export interface LayerStyle {
  type: StyleType;
  fillColor?: string | string[];
  strokeColor?: string | string[];
  strokeWidth?: number;
  opacity?: number;
  radius?: number;
  
  // Data-driven styling
  property?: string;
  breaks?: number[];
  colors?: string[];
  categories?: { value: string | number; color: string; label?: string }[];
}

export interface MapLayer {
  id: string;
  name: string;
  type: LayerType;
  visible: boolean;
  opacity: number;
  zIndex: number;
  
  // Data source
  datasetId?: string;
  query?: string;
  
  // Geometry info
  geometryType?: GeometryType;
  geometryColumn?: string;
  
  // Styling
  style: LayerStyle;
  labelConfig?: LabelConfig;
  
  // Metadata
  featureCount?: number;
  bounds?: [number, number, number, number];
  
  // GeoJSON data (loaded from backend)
  data?: GeoJSON.FeatureCollection;
}

export interface MapView {
  center: [number, number];
  zoom: number;
  bearing: number;
  pitch: number;
}

export interface MapStudioState {
  id: string;
  name: string;
  description?: string;
  basemap: string;
  view: MapView;
  layers: MapLayer[];
  selectedLayerId: string | null;
  showLayerPanel: boolean;
  showStyleEditor: boolean;
  createdAt: Date;
  updatedAt: Date;
}

export interface Dataset {
  id: string;
  name: string;
  table_name: string;
  geometry_column: string;
  geometry_type: GeometryType;
  srid: number;
  feature_count: number;
  bounds?: {
    min_x: number;
    min_y: number;
    max_x: number;
    max_y: number;
  };
  columns: { name: string; data_type: string; nullable: boolean }[];
  created_at: string;
  updated_at: string;
}

// Color ramps for graduated/choropleth styling
export const COLOR_RAMPS: Record<string, string[]> = {
  'Blues': ['#f7fbff', '#deebf7', '#c6dbef', '#9ecae1', '#6baed6', '#4292c6', '#2171b5', '#08519c', '#08306b'],
  'Greens': ['#f7fcf5', '#e5f5e0', '#c7e9c0', '#a1d99b', '#74c476', '#41ab5d', '#238b45', '#006d2c', '#00441b'],
  'Reds': ['#fff5f0', '#fee0d2', '#fcbba1', '#fc9272', '#fb6a4a', '#ef3b2c', '#cb181d', '#a50f15', '#67000d'],
  'Oranges': ['#fff5eb', '#fee6ce', '#fdd0a2', '#fdae6b', '#fd8d3c', '#f16913', '#d94801', '#a63603', '#7f2704'],
  'Purples': ['#fcfbfd', '#efedf5', '#dadaeb', '#bcbddc', '#9e9ac8', '#807dba', '#6a51a3', '#54278f', '#3f007d'],
  'Viridis': ['#440154', '#482878', '#3e4989', '#31688e', '#26828e', '#1f9e89', '#35b779', '#6ece58', '#b5de2b', '#fde725'],
  'Spectral': ['#d53e4f', '#f46d43', '#fdae61', '#fee08b', '#e6f598', '#abdda4', '#66c2a5', '#3288bd', '#5e4fa2'],
  'RdYlGn': ['#d7191c', '#fdae61', '#ffffbf', '#a6d96a', '#1a9641']
};

// Default styles for different geometry types
export const DEFAULT_STYLES: Record<GeometryType, LayerStyle> = {
  'Point': {
    type: 'simple',
    fillColor: '#4a90e2',
    strokeColor: '#2171b5',
    strokeWidth: 1,
    opacity: 0.8,
    radius: 5
  },
  'MultiPoint': {
    type: 'simple',
    fillColor: '#4a90e2',
    strokeColor: '#2171b5',
    strokeWidth: 1,
    opacity: 0.8,
    radius: 5
  },
  'LineString': {
    type: 'simple',
    strokeColor: '#e74c3c',
    strokeWidth: 2,
    opacity: 0.9
  },
  'MultiLineString': {
    type: 'simple',
    strokeColor: '#e74c3c',
    strokeWidth: 2,
    opacity: 0.9
  },
  'Polygon': {
    type: 'simple',
    fillColor: '#2ecc71',
    strokeColor: '#27ae60',
    strokeWidth: 1,
    opacity: 0.6
  },
  'MultiPolygon': {
    type: 'simple',
    fillColor: '#2ecc71',
    strokeColor: '#27ae60',
    strokeWidth: 1,
    opacity: 0.6
  }
};

// Basemap presets
export const BASEMAPS: Basemap[] = [
  {
    id: 'osm',
    label: 'OpenStreetMap',
    url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
    attribution: '© OpenStreetMap contributors',
    maxzoom: 19
  },
  {
    id: 'osm-light',
    label: 'OSM Light',
    url: 'https://a.basemaps.cartocdn.com/light_all/{z}/{x}/{y}.png',
    attribution: '© OpenStreetMap © CARTO',
    maxzoom: 20
  },
  {
    id: 'osm-dark',
    label: 'OSM Dark',
    url: 'https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png',
    attribution: '© OpenStreetMap © CARTO',
    maxzoom: 20
  },
  {
    id: 'satellite',
    label: 'Satellite',
    url: 'https://mt1.google.com/vt/lyrs=s&x={x}&y={y}&z={z}',
    attribution: '© Google',
    maxzoom: 21
  },
  {
    id: 'terrain',
    label: 'Terrain',
    url: 'https://mt1.google.com/vt/lyrs=p&x={x}&y={y}&z={z}',
    attribution: '© Google',
    maxzoom: 21
  },
  {
    id: 'none',
    label: 'None (Blank)',
    url: '',
    attribution: ''
  }
];
