/**
 * deck.gl integration service for rendering geometry data
 * 
 * Uses @deck.gl/geo-layers GeoArrow layers for zero-copy Arrow → GPU rendering.
 * Falls back to manual WKB parsing only when GeoArrow layers can't be used.
 */

import { MapboxOverlay } from '@deck.gl/mapbox';
import { ScatterplotLayer, PathLayer, PolygonLayer } from '@deck.gl/layers';
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

class DeckGLService {
  private overlay: MapboxOverlay | null = null;
  private layers: Map<string, any> = new Map();
  private layerData: Map<string, LayerData> = new Map();
  private map: MapLibreMap | null = null;

  init(map: MapLibreMap): void {
    if (this.overlay) return;
    this.map = map;
    this.overlay = new MapboxOverlay({ interleaved: true, layers: [] });
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
    const isGeoArrowByMeta = extName.startsWith('geoarrow.');

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

  private updateOverlay(): void {
    if (!this.overlay) return;
    this.overlay.setProps({ layers: Array.from(this.layers.values()) });
  }

  dispose(): void {
    if (this.overlay && this.map) this.map.removeControl(this.overlay as any);
    this.overlay = null;
    this.layers.clear();
    this.layerData.clear();
    this.map = null;
  }
}

export const deckglService = new DeckGLService();
