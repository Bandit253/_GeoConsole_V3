import type { Dataset } from '../types/mapStudio';
import { tableFromIPC } from 'apache-arrow';
import { duckdbService } from './duckdb';

const API_BASE = '/api';

// Default TTL for cached datasets (1 hour)
const DEFAULT_CACHE_TTL_MS = 60 * 60 * 1000;

// Dataset API
export async function listDatasets(): Promise<Dataset[]> {
  const res = await fetch(`${API_BASE}/datasets`);
  if (!res.ok) throw new Error(`Failed to list datasets: ${res.statusText}`);
  const data = await res.json();
  return data.datasets;
}

export async function uploadDataset(file: File): Promise<Dataset> {
  const formData = new FormData();
  formData.append('file', file);

  const res = await fetch(`${API_BASE}/datasets`, {
    method: 'POST',
    body: formData
  });

  if (!res.ok) {
    const error = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(error.error || 'Failed to upload dataset');
  }

  return res.json();
}

export async function deleteDataset(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/datasets/${id}`, {
    method: 'DELETE'
  });
  if (!res.ok) throw new Error(`Failed to delete dataset: ${res.statusText}`);
}

// Spatial Data API

export async function getDatasetBounds(datasetId: string): Promise<{ min_x: number; min_y: number; max_x: number; max_y: number }> {
  const res = await fetch(`${API_BASE}/datasets/${datasetId}/bounds`);
  if (!res.ok) throw new Error(`Failed to fetch bounds: ${res.statusText}`);
  return res.json();
}

// ============================================================================
// DuckDB WASM Cache API
// ============================================================================

/**
 * Check if a dataset is cached locally
 */
export function isDatasetCached(datasetId: string): boolean {
  return duckdbService.isCached(datasetId);
}

/**
 * Get cache status for a dataset
 */
export function getDatasetCacheInfo(datasetId: string) {
  return duckdbService.getCacheEntry(datasetId);
}

/**
 * Get all cached datasets
 */
export function getCachedDatasets() {
  return duckdbService.getCachedDatasets();
}

/**
 * Remove a dataset from local cache
 */
export async function removeFromCache(datasetId: string): Promise<void> {
  await duckdbService.removeDataset(datasetId);
}

/**
 * Clear all cached datasets
 */
export async function clearCache(): Promise<void> {
  await duckdbService.clearCache();
}

/**
 * Update TTL for a cached dataset
 */
export async function updateCacheTTL(datasetId: string, ttlMs: number): Promise<void> {
  await duckdbService.updateTTL(datasetId, ttlMs);
}

/**
 * Clean expired cache entries
 */
export async function cleanExpiredCache(): Promise<string[]> {
  return await duckdbService.cleanExpired();
}

/**
 * Fetch raw Arrow IPC bytes from server
 */
export async function fetchArrowIPC(datasetId: string, bbox?: [number, number, number, number]): Promise<ArrayBuffer> {
  const params = new URLSearchParams();
  if (bbox) params.set('bbox', bbox.join(','));
  const qs = params.toString();
  const url = `${API_BASE}/datasets/${datasetId}/features${qs ? '?' + qs : ''}`;
  const res = await fetch(url);
  
  if (!res.ok) throw new Error(`Failed to fetch features: ${res.statusText}`);
  
  const arrayBuffer = await res.arrayBuffer();
  
  if (arrayBuffer.byteLength === 0) {
    throw new Error('Empty Arrow IPC response');
  }

  return arrayBuffer;
}

/**
 * Get Arrow Table for deck.gl rendering via local DuckDB WASM cache
 * Flow: Check cache → Fetch from server if needed → Store in cache → Query from cache
 */
// Track in-flight loads to prevent duplicate concurrent fetches
const _loadingDatasets = new Map<string, Promise<void>>();

export async function getArrowTableForDeckGL(
  datasetId: string,
  geometryType: string,
  ttlMs: number = DEFAULT_CACHE_TTL_MS,
  bbox?: [number, number, number, number]
): Promise<import('apache-arrow').Table> {
  const { duckdbService } = await import('./duckdb');
  
  // Check if cached in local DuckDB WASM
  if (!duckdbService.isCached(datasetId)) {
    // Deduplicate concurrent loads for the same dataset
    if (!_loadingDatasets.has(datasetId)) {
      const loadPromise = (async () => {
        console.log(`Dataset ${datasetId} not cached, fetching from server...`);
        const arrowBuffer = await fetchArrowIPC(datasetId, bbox);
        await duckdbService.loadFromArrowIPC(datasetId, arrowBuffer, geometryType, ttlMs);
        console.log(`Cached dataset ${datasetId} in local DuckDB WASM`);
      })();
      _loadingDatasets.set(datasetId, loadPromise);
      try {
        await loadPromise;
      } finally {
        _loadingDatasets.delete(datasetId);
      }
    } else {
      console.log(`Dataset ${datasetId} already loading, waiting...`);
      await _loadingDatasets.get(datasetId);
    }
  } else {
    console.log(`Using cached dataset ${datasetId} from local DuckDB WASM`);
  }

  // Query from local cache
  const table = await duckdbService.queryAsArrowTable(datasetId);
  console.log(`Loaded Arrow Table for ${datasetId}: ${table.numRows} rows, ${table.numCols} columns`);
  
  return table;
}

/**
 * Query cached dataset with a SQL WHERE filter, returning Arrow Table for deck.gl.
 * Runs entirely in browser DuckDB WASM — no server round-trip.
 */
export async function queryFilteredArrowTable(
  datasetId: string,
  whereClause?: string
): Promise<import('apache-arrow').Table> {
  const { duckdbService } = await import('./duckdb');
  const table = await duckdbService.queryWithFilter(datasetId, whereClause);
  console.log(`Filtered query for ${datasetId}: ${table.numRows} rows${whereClause ? ` (WHERE ${whereClause})` : ''}`);
  return table;
}

/**
 * Get unique values for a column from cached dataset (for categorized styling).
 */
export async function getColumnUniqueValues(
  datasetId: string,
  columnName: string,
  limit = 100
): Promise<(string | number)[]> {
  const { duckdbService } = await import('./duckdb');
  return duckdbService.getColumnUniqueValues(datasetId, columnName, limit);
}

/**
 * Get min/max range for a numeric column from cached dataset (for graduated styling).
 */
export async function getColumnRange(
  datasetId: string,
  columnName: string
): Promise<{ min: number; max: number } | null> {
  const { duckdbService } = await import('./duckdb');
  return duckdbService.getColumnRange(datasetId, columnName);
}

// Routing API
export interface RouteRequest {
  locations: [number, number][];
  costing: 'auto' | 'bicycle' | 'pedestrian';
  units?: 'kilometers' | 'miles';
}

export async function calculateRoute(request: RouteRequest): Promise<any> {
  const res = await fetch(`${API_BASE}/routing/route`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request)
  });

  if (!res.ok) {
    const error = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(error.error || 'Routing failed');
  }

  return res.json();
}

export interface IsochroneRequest {
  locations: [number, number][];
  costing: 'auto' | 'bicycle' | 'pedestrian';
  contours: { time: number; color: string }[];
}

export async function calculateIsochrone(request: IsochroneRequest): Promise<any> {
  const res = await fetch(`${API_BASE}/routing/isochrone`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request)
  });

  if (!res.ok) {
    const error = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(error.error || 'Isochrone calculation failed');
  }

  return res.json();
}

// Map Configuration API
export interface MapConfigRequest {
  name: string;
  description?: string;
  basemap?: string;
  view?: {
    center: [number, number];
    zoom: number;
    bearing: number;
    pitch: number;
  };
}

export async function createMap(request: MapConfigRequest): Promise<any> {
  const res = await fetch(`${API_BASE}/maps`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request)
  });

  if (!res.ok) throw new Error(`Failed to create map: ${res.statusText}`);
  return res.json();
}

export async function listMaps(): Promise<any[]> {
  const res = await fetch(`${API_BASE}/maps`);
  if (!res.ok) throw new Error(`Failed to list maps: ${res.statusText}`);
  const data = await res.json();
  return data.maps;
}
