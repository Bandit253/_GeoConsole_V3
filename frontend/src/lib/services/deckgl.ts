/**
 * deck.gl integration service for rendering geometry data
 * 
 * Uses @deck.gl/geo-layers GeoArrow layers for zero-copy Arrow → GPU rendering.
 * Falls back to manual WKB parsing only when GeoArrow layers can't be used.
 */

import { MapboxOverlay } from '@deck.gl/mapbox';
import { ScatterplotLayer, PathLayer, PolygonLayer, TextLayer } from '@deck.gl/layers';
import Supercluster from 'supercluster';
import { GeoArrowScatterplotLayer, GeoArrowPathLayer, GeoArrowPolygonLayer } from '@geoarrow/deck.gl-layers';
import type { Table } from 'apache-arrow';
import type { Map as MapLibreMap } from 'maplibre-gl';

// ============================================================================
// WKB Binary Parser (fallback only — used for GeoJSON export, not rendering)
// ============================================================================

/**
 * Parse WKB (Well-Known Binary) geometry into GeoJSON-like structure.
 */
function parseWKB(wkb: Uint8Array): { type: string; coordinates: any } | null {
  if (!wkb || wkb.length < 5) return null;
  const view = new DataView(wkb.buffer, wkb.byteOffset, wkb.byteLength);
  const result = readWKBGeometry(view, 0);
  return result?.geometry ?? null;
}

function readWKBGeometry(view: DataView, offset: number): { geometry: { type: string; coordinates: any }; offset: number } | null {
  if (offset + 5 > view.byteLength) return null;

  const byteOrder = view.getUint8(offset);
  const le = byteOrder === 1;
  offset += 1;

  let geomType = view.getUint32(offset, le);
  offset += 4;

  let hasSRID = false;
  if (geomType & 0x20000000) {
    hasSRID = true;
    geomType &= ~0x20000000;
  }
  // ISO WKB: Z = 0x80000000, M = 0x40000000
  const hasZIso = !!(geomType & 0x80000000);
  const hasMIso = !!(geomType & 0x40000000);
  geomType &= 0x0000FFFF;

  if (hasSRID) offset += 4;

  // OGC/EWKB: Z = +1000, M = +2000, ZM = +3000
  const hasZOgc = geomType >= 1000 && geomType < 2000;
  const hasMOgc = geomType >= 2000 && geomType < 3000;
  const hasZMOgc = geomType >= 3000;
  const hasZ = hasZIso || hasZOgc || hasZMOgc;
  const hasM = hasMIso || hasMOgc || hasZMOgc;
  const baseType = geomType % 1000;
  const dims = 2 + (hasZ ? 1 : 0) + (hasM ? 1 : 0);

  switch (baseType) {
    case 0: return null; // Empty/unknown
    case 1: { const coords = readPoint(view, offset, le, dims); return { geometry: { type: 'Point', coordinates: coords.point }, offset: coords.offset }; }
    case 2: { const line = readLineString(view, offset, le, dims); return { geometry: { type: 'LineString', coordinates: line.coords }, offset: line.offset }; }
    case 3: { const poly = readPolygon(view, offset, le, dims); return { geometry: { type: 'Polygon', coordinates: poly.rings }, offset: poly.offset }; }
    case 4: { const count = view.getUint32(offset, le); offset += 4; const points: number[][] = []; for (let i = 0; i < count; i++) { const sub = readWKBGeometry(view, offset); if (!sub) return null; points.push(sub.geometry.coordinates); offset = sub.offset; } return { geometry: { type: 'MultiPoint', coordinates: points }, offset }; }
    case 5: { const count = view.getUint32(offset, le); offset += 4; const lines: number[][][] = []; for (let i = 0; i < count; i++) { const sub = readWKBGeometry(view, offset); if (!sub) return null; lines.push(sub.geometry.coordinates); offset = sub.offset; } return { geometry: { type: 'MultiLineString', coordinates: lines }, offset }; }
    case 6: { const count = view.getUint32(offset, le); offset += 4; const polys: number[][][][] = []; for (let i = 0; i < count; i++) { const sub = readWKBGeometry(view, offset); if (!sub) return null; polys.push(sub.geometry.coordinates); offset = sub.offset; } return { geometry: { type: 'MultiPolygon', coordinates: polys }, offset }; }
    case 7: { // GeometryCollection — return first sub-geometry
      const count = view.getUint32(offset, le); offset += 4;
      for (let i = 0; i < count; i++) { const sub = readWKBGeometry(view, offset); if (!sub) return null; offset = sub.offset; if (i === 0 && sub.geometry) return { geometry: sub.geometry, offset }; }
      return null;
    }
    default: console.warn(`Unsupported WKB geometry type: ${baseType}`); return null;
  }
}

function readPoint(view: DataView, offset: number, le: boolean, dims: number): { point: number[]; offset: number } {
  const x = view.getFloat64(offset, le); offset += 8;
  const y = view.getFloat64(offset, le); offset += 8;
  // Skip Z/M/ZM — deck.gl interprets [x,y,z] as elevation
  for (let d = 2; d < dims; d++) offset += 8;
  return { point: [x, y], offset };
}

function readLineString(view: DataView, offset: number, le: boolean, dims: number): { coords: number[][]; offset: number } {
  const numPoints = view.getUint32(offset, le); offset += 4;
  const coords: number[][] = [];
  for (let i = 0; i < numPoints; i++) { const p = readPoint(view, offset, le, dims); coords.push(p.point); offset = p.offset; }
  return { coords, offset };
}

function readPolygon(view: DataView, offset: number, le: boolean, dims: number): { rings: number[][][]; offset: number } {
  const numRings = view.getUint32(offset, le); offset += 4;
  const rings: number[][][] = [];
  for (let i = 0; i < numRings; i++) { const ring = readLineString(view, offset, le, dims); rings.push(ring.coords); offset = ring.offset; }
  return { rings, offset };
}

export { parseWKB };

// ============================================================================
// Style Types and Helpers
// ============================================================================

export interface DeckLayerStyle {
  fillColor?: [number, number, number, number];
  strokeColor?: [number, number, number, number];
  strokeWidth?: number;
  radius?: number;
  opacity?: number;
  styleType?: 'simple' | 'graduated' | 'categorized';
  property?: string;
  colors?: string[];
  breaks?: number[];
  categories?: { value: string | number; color: string }[];
  cluster?: boolean;
  clusterRadius?: number;
  clusterMaxZoom?: number;
}

function hexToRgba(hex: string, alpha = 255): [number, number, number, number] {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  if (result) return [parseInt(result[1], 16), parseInt(result[2], 16), parseInt(result[3], 16), alpha];
  return [100, 100, 100, alpha];
}

function interpolateColor(value: number, colors: string[], alpha = 255): [number, number, number, number] {
  if (colors.length === 0) return [100, 100, 100, alpha];
  if (colors.length === 1) return hexToRgba(colors[0], alpha);
  const t = Math.max(0, Math.min(1, value));
  const segmentCount = colors.length - 1;
  const segment = Math.min(Math.floor(t * segmentCount), segmentCount - 1);
  const segmentT = (t * segmentCount) - segment;
  const c1 = hexToRgba(colors[segment], alpha);
  const c2 = hexToRgba(colors[segment + 1], alpha);
  return [
    Math.round(c1[0] + (c2[0] - c1[0]) * segmentT),
    Math.round(c1[1] + (c2[1] - c1[1]) * segmentT),
    Math.round(c1[2] + (c2[2] - c1[2]) * segmentT),
    alpha
  ];
}

const CATEGORY_COLORS = [
  '#e41a1c', '#377eb8', '#4daf4a', '#984ea3', '#ff7f00',
  '#ffff33', '#a65628', '#f781bf', '#999999', '#66c2a5',
  '#fc8d62', '#8da0cb', '#e78ac3', '#a6d854', '#ffd92f'
];

function getCategoryColor(index: number, alpha = 255): [number, number, number, number] {
  return hexToRgba(CATEGORY_COLORS[index % CATEGORY_COLORS.length], alpha);
}

// ============================================================================
// DeckGL Service
// ============================================================================

interface LayerData {
  // For GeoArrow path: store the original Arrow table
  arrowTable?: Table;
  // For fallback path: store parsed features
  features?: any[];
  geometryType: string;
  style: DeckLayerStyle;
  visible: boolean;
}

export interface PickedFeatureInfo {
  layerId: string;
  properties: Record<string, any>;
  coordinate: [number, number];
}

class DeckGLService {
  private overlay: MapboxOverlay | null = null;
  private layers: Map<string, any | any[]> = new Map();
  private layerData: Map<string, LayerData> = new Map();
  private map: MapLibreMap | null = null;
  private _onFeatureClick: ((info: PickedFeatureInfo) => void) | null = null;
  private clusterIndices: Map<string, Supercluster> = new Map();
  private clusterSettings: Map<string, { radius: number; maxZoom: number }> = new Map();

  /** Register a callback for feature click events */
  set onFeatureClick(cb: ((info: PickedFeatureInfo) => void) | null) {
    this._onFeatureClick = cb;
  }

  init(map: MapLibreMap): void {
    if (this.overlay) return;
    this.map = map;
    this.overlay = new MapboxOverlay({
      interleaved: true,
      layers: [],
      onClick: (info: any) => {
        if (!info.object || !this._onFeatureClick) return;
        const coordinate = info.coordinate as [number, number];
        const layerId = info.layer?.id || '';
        let properties: Record<string, any> = {};

        // Fallback layers store properties on the object directly
        if (info.object.properties) {
          properties = { ...info.object.properties };
        } else if (info.index !== undefined && info.index >= 0) {
          // GeoArrow path: read row from the Arrow table
          const data = this.layerData.get(layerId);
          if (data?.arrowTable) {
            const table = data.arrowTable;
            for (const field of table.schema.fields) {
              if (field.name === 'geometry' || field.name === '__geometry') continue;
              const col = table.getChild(field.name);
              if (col) properties[field.name] = col.get(info.index);
            }
          }
        }

        if (Object.keys(properties).length > 0) {
          this._onFeatureClick({ layerId, properties, coordinate });
        }
      },
    });
    map.addControl(this.overlay as any);
    console.log('deck.gl overlay initialized');
  }

  /**
   * Add or update a layer from an Arrow Table with WKB geometry.
   * Tries GeoArrow layer first (zero-copy), falls back to manual parsing.
   */
  addArrowLayer(
    layerId: string,
    table: Table,
    geometryType: string,
    style: DeckLayerStyle = {},
    visible = true
  ): void {
    if (!this.overlay) {
      console.error('deck.gl overlay not initialized');
      return;
    }

    const geomCol = table.getChild('geometry');
    if (!geomCol) {
      console.error('No geometry column in Arrow table');
      return;
    }

    // Cluster path for Point layers
    if (style.cluster && geometryType.toLowerCase().includes('point')) {
      const features = this.parseGeoJsonGeometry(table);
      const data: LayerData = { arrowTable: table, features, geometryType, style, visible };
      this.layerData.set(layerId, data);
      this.ensureClusterIndex(layerId, data, style);
      const clusterLayers = this.buildClusterLayers(layerId, style, visible);
      this.layers.set(layerId, clusterLayers);
      this.updateOverlay();
      console.log(`Added cluster layer: ${layerId} (${geometryType}, ${features.length} features)`);
      return;
    }

    // Try GeoArrow path first (zero-copy Arrow → GPU)
    const geoArrowLayer = this.tryCreateGeoArrowLayer(layerId, table, geometryType, style, visible);
    if (geoArrowLayer) {
      this.layerData.set(layerId, { arrowTable: table, geometryType, style, visible });
      this.layers.set(layerId, geoArrowLayer);
      this.updateOverlay();
      console.log(`Added GeoArrow layer: ${layerId} (${geometryType}, ${table.numRows} rows, zero-copy)`);
      return;
    }

    // Fallback: manual WKB parsing
    console.log(`GeoArrow not available for ${geometryType}, falling back to manual WKB parsing`);
    const features = this.parseGeoJsonGeometry(table);
    if (features.length === 0) {
      console.warn(`No valid features parsed for layer ${layerId}`);
      return;
    }

    this.layerData.set(layerId, { features, geometryType, style, visible });
    const layer = this.createFallbackLayer(layerId, features, geometryType, style, visible);
    this.layers.set(layerId, layer);
    this.updateOverlay();
    console.log(`Added deck.gl layer (fallback): ${layerId} (${geometryType}, ${features.length} features)`);
  }

  /**
   * Try to create a GeoArrow layer (zero-copy Arrow → GPU).
   * Returns null if the geometry type isn't supported by GeoArrow layers.
   */
  private tryCreateGeoArrowLayer(
    layerId: string,
    table: Table,
    geometryType: string,
    style: DeckLayerStyle,
    visible: boolean
  ): any | null {
    const opacity = style.opacity ?? 0.8;
    const defaultFillColor = style.fillColor ?? hexToRgba('#3388ff', Math.round(opacity * 255));
    const strokeColor = style.strokeColor ?? hexToRgba('#2171b5', 255);
    const geomTypeLower = geometryType.toLowerCase();

    // Pre-validate: GeoArrow layers require native Arrow struct types
    // (e.g. geoarrow.point = Struct<x: Float64, y: Float64>) with
    // proper GeoArrow extension metadata. DuckDB's internal GEOMETRY
    // type may appear struct-like but isn't valid GeoArrow — passing it
    // causes async errors in renderLayers() on every animation frame
    // that the try/catch below cannot catch.
    const geomCol = table.getChild('geometry');
    if (!geomCol) return null;

    const geomField = table.schema.fields.find(f => f.name === 'geometry');
    const extName = geomField?.metadata?.get('ARROW:extension:name') ?? '';
    // geoarrow.wkb is still WKB binary — GeoArrow layers require native struct types
    const isGeoArrowByMeta = extName.startsWith('geoarrow.') && extName !== 'geoarrow.wkb';

    // Also check struct children: GeoArrow Point has x/y Float children
    const arrowType = geomCol.type;
    const children = (arrowType as any)?.children;
    const childNames = Array.isArray(children) ? children.map((c: any) => c?.name?.toLowerCase()) : [];
    const isGeoArrowByStruct =
      childNames.includes('x') && childNames.includes('y');

    if (!isGeoArrowByMeta && !isGeoArrowByStruct) {
      console.log(
        `Geometry column is not GeoArrow-native (type: ${arrowType}, ext: "${extName}"), skipping GeoArrow path`
      );
      return null;
    }

    try {
      if (geomTypeLower.includes('point')) {
        return new GeoArrowScatterplotLayer({
          id: layerId,
          data: table,
          getPosition: table.getChild('geometry')!,
          getFillColor: defaultFillColor,
          getLineColor: strokeColor,
          getRadius: style.radius ?? 5,
          lineWidthMinPixels: style.strokeWidth ?? 1,
          pickable: true,
          visible,
          opacity,
        });
      } else if (geomTypeLower.includes('line')) {
        return new GeoArrowPathLayer({
          id: layerId,
          data: table,
          getPath: table.getChild('geometry')!,
          getColor: strokeColor,
          getWidth: style.strokeWidth ?? 2,
          widthUnits: 'pixels' as const,
          widthMinPixels: 1,
          pickable: true,
          visible,
          opacity,
        });
      } else if (geomTypeLower.includes('polygon')) {
        return new GeoArrowPolygonLayer({
          id: layerId,
          data: table,
          getPolygon: table.getChild('geometry')!,
          getFillColor: defaultFillColor,
          getLineColor: strokeColor,
          getLineWidth: style.strokeWidth ?? 2,
          lineWidthUnits: 'pixels' as const,
          lineWidthMinPixels: 1,
          stroked: true,
          filled: true,
          pickable: true,
          visible,
          opacity,
          extruded: false,
        });
      }
    } catch (e) {
      console.warn(`GeoArrow layer creation failed for ${layerId}:`, e);
    }

    return null;
  }

  /**
   * Update layer style — recreates layer with new style
   */
  updateLayerStyle(layerId: string, newStyle: DeckLayerStyle): void {
    const data = this.layerData.get(layerId);
    if (!data) {
      console.warn(`updateLayerStyle: no data for ${layerId}`);
      return;
    }

    const mergedStyle: DeckLayerStyle = { ...data.style, ...newStyle };
    this.layerData.set(layerId, { ...data, style: mergedStyle });

    // Cluster path
    if (mergedStyle.cluster && data.geometryType.toLowerCase().includes('point')) {
      if (!data.features && data.arrowTable) {
        data.features = this.parseGeoJsonGeometry(data.arrowTable);
        this.layerData.set(layerId, { ...data, features: data.features });
      }
      this.ensureClusterIndex(layerId, { ...data, style: mergedStyle }, mergedStyle);
      const clusterLayers = this.buildClusterLayers(layerId, mergedStyle, data.visible);
      this.layers.set(layerId, clusterLayers);
      this.updateOverlay();
      return;
    }

    // Remove cluster index if clustering was disabled
    if (this.clusterIndices.has(layerId)) {
      this.clusterIndices.delete(layerId);
      this.clusterSettings.delete(layerId);
    }

    let layer: any;
    if (data.arrowTable) {
      // For data-driven styling with GeoArrow, we need the fallback path
      // since GeoArrow layers don't support per-feature color accessors from JS
      if (mergedStyle.styleType && mergedStyle.styleType !== 'simple' && mergedStyle.property) {
        // Parse features for data-driven styling (one-time cost)
        if (!data.features) {
          data.features = this.parseGeoJsonGeometry(data.arrowTable);
        }
        layer = this.createFallbackLayer(layerId, data.features!, data.geometryType, mergedStyle, data.visible);
      } else {
        layer = this.tryCreateGeoArrowLayer(layerId, data.arrowTable, data.geometryType, mergedStyle, data.visible);
        if (!layer && data.features) {
          layer = this.createFallbackLayer(layerId, data.features, data.geometryType, mergedStyle, data.visible);
        }
      }
    } else if (data.features) {
      layer = this.createFallbackLayer(layerId, data.features, data.geometryType, mergedStyle, data.visible);
    }

    if (layer) {
      this.layers.set(layerId, layer);
      this.updateOverlay();
    }
  }

  setLayerVisibility(layerId: string, visible: boolean): void {
    const data = this.layerData.get(layerId);
    if (!data) return;
    this.layerData.set(layerId, { ...data, visible });
    this.updateLayerStyle(layerId, { ...data.style });
  }

  removeLayer(layerId: string): void {
    this.layers.delete(layerId);
    this.layerData.delete(layerId);
    this.clusterIndices.delete(layerId);
    this.clusterSettings.delete(layerId);
    this.updateOverlay();
  }

  reorderLayers(layerIds: string[]): void {
    const reordered = new Map<string, any>();
    for (const id of layerIds) {
      const layer = this.layers.get(id);
      if (layer) reordered.set(id, layer);
    }
    this.layers = reordered;
    this.updateOverlay();
  }

  // ========================================================================
  // Fallback: manual WKB parsing + standard deck.gl layers
  // ========================================================================

  private createFallbackLayer(
    layerId: string,
    features: any[],
    geometryType: string,
    style: DeckLayerStyle,
    visible: boolean
  ): any {
    const opacity = style.opacity ?? 0.8;
    const defaultFillColor = style.fillColor ?? hexToRgba('#3388ff', Math.round(opacity * 255));
    const strokeColor = style.strokeColor ?? hexToRgba('#2171b5', 255);
    const geomTypeLower = geometryType.toLowerCase();
    const getFillColorAccessor = this.buildColorAccessor(features, style, opacity);

    if (geomTypeLower.includes('point')) {
      return new ScatterplotLayer({
        id: layerId, data: features,
        getPosition: (d: any) => d.coordinates,
        getFillColor: getFillColorAccessor || defaultFillColor,
        getLineColor: strokeColor,
        getRadius: style.radius ?? 5,
        lineWidthMinPixels: style.strokeWidth ?? 1,
        pickable: true, visible, opacity,
        updateTriggers: { getFillColor: [style.property, style.colors, style.styleType] }
      });
    } else if (geomTypeLower.includes('line')) {
      return new PathLayer({
        id: layerId, data: features,
        getPath: (d: any) => d.coordinates,
        getColor: getFillColorAccessor || strokeColor,
        getWidth: style.strokeWidth ?? 2,
        widthUnits: 'pixels' as const, widthMinPixels: 1,
        pickable: true, visible, opacity,
        updateTriggers: { getColor: [style.property, style.colors, style.styleType] }
      });
    } else {
      return new PolygonLayer({
        id: layerId, data: features,
        getPolygon: (d: any) => d.coordinates,
        getFillColor: getFillColorAccessor || defaultFillColor,
        getLineColor: strokeColor,
        getLineWidth: style.strokeWidth ?? 2,
        lineWidthUnits: 'pixels' as const, lineWidthMinPixels: 1,
        stroked: true, filled: true, pickable: true, visible, opacity, extruded: false,
        updateTriggers: { getFillColor: [style.property, style.colors, style.styleType] }
      });
    }
  }

  private buildColorAccessor(
    features: any[],
    style: DeckLayerStyle,
    opacity: number
  ): ((d: any) => [number, number, number, number]) | null {
    if (!style.property || style.styleType === 'simple' || !style.colors?.length) return null;

    const alpha = Math.round(opacity * 255);
    const property = style.property;
    const colors = style.colors;
    const values = features.map(f => f.properties?.[property]).filter(v => v !== undefined && v !== null);
    if (values.length === 0) return null;

    const isNumeric = values.every(v => typeof v === 'number' || !isNaN(Number(v)));

    if (isNumeric && style.styleType === 'graduated') {
      let minVal = Infinity, maxVal = -Infinity;
      for (const v of values) { const num = Number(v); if (num < minVal) minVal = num; if (num > maxVal) maxVal = num; }
      const range = maxVal - minVal || 1;
      return (d: any) => {
        const val = d.properties?.[property];
        if (val === undefined || val === null) return [100, 100, 100, alpha];
        return interpolateColor((Number(val) - minVal) / range, colors, alpha);
      };
    } else {
      const uniqueValues = [...new Set(values.map(v => String(v)))];
      const valueToColorIndex = new Map<string, number>();
      uniqueValues.forEach((v, i) => valueToColorIndex.set(v, i));
      return (d: any) => {
        const val = d.properties?.[property];
        if (val === undefined || val === null) return [100, 100, 100, alpha];
        const idx = valueToColorIndex.get(String(val)) ?? 0;
        if (colors.length >= uniqueValues.length) return hexToRgba(colors[Math.min(idx, colors.length - 1)], alpha);
        return getCategoryColor(idx, alpha);
      };
    }
  }

  /**
   * Parse WKB geometry from Arrow table (fallback path)
   */
  private parseGeoJsonGeometry(table: Table): any[] {
    const features: any[] = [];
    const geomCol = table.getChild('geometry');
    if (!geomCol) return features;

    const propColumns: { name: string; col: any }[] = [];
    for (const field of table.schema.fields) {
      if (field.name !== 'geometry') {
        const col = table.getChild(field.name);
        if (col) propColumns.push({ name: field.name, col });
      }
    }

    for (let i = 0; i < table.numRows; i++) {
      const geomValue = geomCol.get(i);
      if (!geomValue) continue;
      try {
        const properties: Record<string, any> = {};
        for (const { name, col } of propColumns) properties[name] = col.get(i);

        let parsed: { type: string; coordinates: any } | null = null;
        if (geomValue instanceof Uint8Array) parsed = parseWKB(geomValue);
        else if (typeof geomValue === 'string') parsed = JSON.parse(geomValue);
        if (!parsed || !parsed.coordinates) continue;

        if (parsed.type === 'MultiPolygon') {
          for (const polygon of parsed.coordinates) features.push({ coordinates: polygon, type: 'Polygon', index: i, properties });
        } else if (parsed.type === 'MultiLineString') {
          for (const line of parsed.coordinates) features.push({ coordinates: line, type: 'LineString', index: i, properties });
        } else if (parsed.type === 'MultiPoint') {
          for (const point of parsed.coordinates) features.push({ coordinates: point, type: 'Point', index: i, properties });
        } else {
          features.push({ coordinates: parsed.coordinates, type: parsed.type, index: i, properties });
        }
      } catch (e) { /* skip invalid */ }
    }
    return features;
  }

  // ========================================================================
  // Clustering
  // ========================================================================

  /** Build or rebuild the supercluster index for a layer, skipping if settings unchanged */
  private ensureClusterIndex(layerId: string, data: LayerData, style: DeckLayerStyle): void {
    const radius = style.clusterRadius ?? 60;
    const maxZoom = style.clusterMaxZoom ?? 16;
    const existing = this.clusterSettings.get(layerId);
    if (this.clusterIndices.has(layerId) && existing?.radius === radius && existing?.maxZoom === maxZoom) return;

    const features = data.features ?? [];
    if (features.length === 0) return;

    const points = features
      .filter(f => Array.isArray(f.coordinates) && !Array.isArray(f.coordinates[0]))
      .map(f => ({
        type: 'Feature' as const,
        geometry: { type: 'Point' as const, coordinates: f.coordinates as [number, number] },
        properties: (f.properties ?? {}) as Record<string, any>
      }));

    const index = new Supercluster({ radius, maxZoom });
    index.load(points);
    this.clusterIndices.set(layerId, index);
    this.clusterSettings.set(layerId, { radius, maxZoom });
  }

  /** Build ScatterplotLayer + TextLayer for the current map viewport/zoom */
  private buildClusterLayers(layerId: string, style: DeckLayerStyle, visible: boolean): any[] {
    const index = this.clusterIndices.get(layerId);
    if (!index || !this.map) return [];

    const zoom = Math.floor(this.map.getZoom());
    const b = this.map.getBounds();
    const clusters = index.getClusters([b.getWest(), b.getSouth(), b.getEast(), b.getNorth()], zoom);

    const opacity = style.opacity ?? 0.8;
    const alpha = Math.round(opacity * 255);
    const baseRadius = style.radius ?? 5;
    const fillColor = style.fillColor ?? hexToRgba('#3388ff', alpha);
    const strokeColor = style.strokeColor ?? hexToRgba('#2171b5', 255);

    const circleLayer = new ScatterplotLayer({
      id: `${layerId}__cluster_circles`,
      data: clusters,
      getPosition: (d: any) => d.geometry.coordinates,
      getRadius: (d: any) => d.properties.cluster
        ? Math.min(40, Math.max(baseRadius * 2, Math.log10(d.properties.point_count + 1) * 10))
        : baseRadius,
      radiusUnits: 'pixels' as const,
      getFillColor: (d: any) => {
        if (!d.properties.cluster) return fillColor as [number, number, number, number];
        const count = d.properties.point_count;
        if (count < 10)  return [255, 237, 160, alpha] as [number, number, number, number];
        if (count < 100) return [254, 178,  76, alpha] as [number, number, number, number];
        return [240, 59, 32, alpha] as [number, number, number, number];
      },
      getLineColor: strokeColor as [number, number, number, number],
      lineWidthMinPixels: 1,
      pickable: true,
      visible,
      updateTriggers: { getRadius: [zoom, baseRadius], getFillColor: [zoom, alpha], getPosition: zoom }
    });

    const labelLayer = new TextLayer({
      id: `${layerId}__cluster_labels`,
      data: clusters.filter((d: any) => d.properties.cluster),
      getPosition: (d: any) => d.geometry.coordinates,
      getText: (d: any) => String(d.properties.point_count_abbreviated ?? d.properties.point_count),
      getSize: 12,
      getColor: [255, 255, 255, 255] as [number, number, number, number],
      getTextAnchor: 'middle' as any,
      getAlignmentBaseline: 'center' as any,
      pickable: false,
      visible,
      updateTriggers: { data: zoom, getPosition: zoom, getText: zoom }
    });

    return [circleLayer, labelLayer];
  }

  /**
   * Re-render all clustered layers at the given zoom level.
   * Call from MapLibre's zoomend event.
   */
  updateClusterZoom(zoom: number): void {
    if (this.clusterIndices.size === 0) return;
    for (const [layerId] of this.clusterIndices) {
      const data = this.layerData.get(layerId);
      if (!data) continue;
      const clusterLayers = this.buildClusterLayers(layerId, data.style, data.visible);
      this.layers.set(layerId, clusterLayers);
    }
    this.updateOverlay();
  }

  private updateOverlay(): void {
    if (!this.overlay) return;
    const allLayers: any[] = [];
    for (const entry of this.layers.values()) {
      if (Array.isArray(entry)) allLayers.push(...entry);
      else allLayers.push(entry);
    }
    this.overlay.setProps({ layers: allLayers });
  }

  dispose(): void {
    if (this.overlay && this.map) this.map.removeControl(this.overlay as any);
    this.overlay = null;
    this.layers.clear();
    this.layerData.clear();
    this.clusterIndices.clear();
    this.clusterSettings.clear();
    this.map = null;
  }
}

export const deckglService = new DeckGLService();
