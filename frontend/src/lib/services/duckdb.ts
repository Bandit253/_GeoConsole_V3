/**
 * DuckDB WASM Service for Browser-side Spatial Data Processing
 * 
 * Uses OPFS (Origin Private File System) for persistent storage so cached
 * datasets survive page refreshes. Falls back to in-memory mode if OPFS
 * is unavailable (e.g. missing COOP/COEP headers).
 * 
 * Arrow IPC buffers are also stored in IndexedDB as a secondary fallback
 * for re-hydrating tables if OPFS storage is lost.
 */

import * as duckdb from '@duckdb/duckdb-wasm';
import type { AsyncDuckDB, AsyncDuckDBConnection } from '@duckdb/duckdb-wasm';
import { tableFromIPC, type Table } from 'apache-arrow';

// Cache entry with TTL metadata
interface CacheEntry {
  datasetId: string;
  tableName: string;
  cachedAt: number;
  ttlMs: number;
  featureCount: number;
  geometryType: string;
}

// Default TTL: 1 hour
const DEFAULT_TTL_MS = 60 * 60 * 1000;

// IndexedDB database name for persistence
const IDB_NAME = 'geoconsole-cache';
const IDB_STORE = 'datasets';
const IDB_ARROW_STORE = 'arrow-buffers';

// OPFS database path
const OPFS_DB_PATH = 'geoconsole.duckdb';

class DuckDBService {
  private db: AsyncDuckDB | null = null;
  private conn: AsyncDuckDBConnection | null = null;
  private initialized = false;
  private initPromise: Promise<void> | null = null;
  private cache: Map<string, CacheEntry> = new Map();
  private persistent = false;

  /**
   * Initialize DuckDB WASM with spatial extension
   */
  async init(): Promise<void> {
    if (this.initialized) return;
    if (this.initPromise) return this.initPromise;

    this.initPromise = this._doInit();
    return this.initPromise;
  }

  /** Whether the database is using OPFS persistent storage */
  get isPersistent(): boolean {
    return this.persistent;
  }

  private async _doInit(): Promise<void> {
    try {
      // Select the best bundle for the browser
      const JSDELIVR_BUNDLES = duckdb.getJsDelivrBundles();
      const bundle = await duckdb.selectBundle(JSDELIVR_BUNDLES);

      const worker_url = URL.createObjectURL(
        new Blob([`importScripts("${bundle.mainWorker}");`], { type: 'text/javascript' })
      );

      const worker = new Worker(worker_url);
      const logger = new duckdb.ConsoleLogger();
      
      this.db = new duckdb.AsyncDuckDB(logger, worker);
      await this.db.instantiate(bundle.mainModule, bundle.pthreadWorker);
      
      URL.revokeObjectURL(worker_url);

      // Try OPFS persistence, fall back to in-memory
      this.persistent = await this.tryOpenPersistent();

      // Open connection
      this.conn = await this.db.connect();

      // Load spatial extension
      await this.conn.query(`INSTALL spatial; LOAD spatial;`);

      // Load cache metadata from IndexedDB
      await this.loadCacheMetadata();

      if (this.persistent) {
        // OPFS mode: tables survive refreshes, just validate they still exist
        await this.validateCachedTables();
      } else {
        // In-memory mode: tables are gone, try to re-hydrate from IndexedDB Arrow buffers
        await this.rehydrateFromIndexedDB();
      }

      this.initialized = true;
      console.log(`DuckDB WASM initialized (${this.persistent ? 'OPFS persistent' : 'in-memory'})`);
    } catch (error) {
      console.error('Failed to initialize DuckDB WASM:', error);
      throw error;
    }
  }

  /**
   * Try to open DuckDB with OPFS persistence.
   * Returns true if successful, false if OPFS is unavailable.
   */
  private async tryOpenPersistent(): Promise<boolean> {
    if (!this.db) return false;
    try {
      // Check for OPFS support (requires COOP/COEP headers + secure context)
      if (typeof globalThis.navigator?.storage?.getDirectory !== 'function') {
        console.log('OPFS not available, using in-memory DuckDB');
        return false;
      }
      // Open with OPFS path
      await this.db.open({ path: OPFS_DB_PATH });
      console.log('DuckDB WASM opened with OPFS persistence');
      return true;
    } catch (e) {
      console.warn('OPFS persistence failed, falling back to in-memory:', e);
      return false;
    }
  }

  /**
   * Check if a dataset is cached locally
   */
  isCached(datasetId: string): boolean {
    const entry = this.cache.get(datasetId);
    if (!entry) return false;
    
    // Check TTL
    if (Date.now() - entry.cachedAt > entry.ttlMs) {
      this.removeDataset(datasetId);
      return false;
    }
    
    return true;
  }

  /**
   * Get cache entry metadata
   */
  getCacheEntry(datasetId: string): CacheEntry | null {
    return this.cache.get(datasetId) || null;
  }

  /**
   * Load dataset from Arrow IPC buffer into local DuckDB
   */
  async loadFromArrowIPC(
    datasetId: string,
    arrowBuffer: ArrayBuffer,
    geometryType: string,
    ttlMs: number = DEFAULT_TTL_MS
  ): Promise<void> {
    await this.init();
    if (!this.db || !this.conn) throw new Error('DuckDB not initialized');

    const tableName = `dataset_${datasetId.replace(/-/g, '_')}`;

    // Drop existing table if present
    await this.conn.query(`DROP TABLE IF EXISTS ${tableName}`);

    // Parse Arrow IPC buffer using apache-arrow
    const arrowTable = tableFromIPC(arrowBuffer);
    
    // Insert Arrow table into DuckDB using insertArrowTable
    await this.conn.insertArrowTable(arrowTable, { name: tableName, create: true });

    // Get feature count
    const countResult = await this.conn.query(`SELECT COUNT(*) as cnt FROM ${tableName}`);
    const featureCount = Number(countResult.toArray()[0]?.cnt || 0);

    // Store cache metadata
    const entry: CacheEntry = {
      datasetId,
      tableName,
      cachedAt: Date.now(),
      ttlMs,
      featureCount,
      geometryType
    };
    this.cache.set(datasetId, entry);

    // Persist metadata + Arrow buffer to IndexedDB (fallback for non-OPFS browsers)
    await this.saveCacheMetadata();
    await this.saveArrowBuffer(datasetId, arrowBuffer);

    console.log(`Cached dataset ${datasetId}: ${featureCount} features, TTL ${ttlMs}ms`);
  }

  /**
   * Query cached dataset and return Arrow Table for MapLibre
   * The table can be used directly with arrow-js for rendering
   */
  async queryAsArrowTable(
    datasetId: string,
    limit?: number,
    offset?: number
  ): Promise<Table> {
    await this.init();
    if (!this.conn) throw new Error('DuckDB not initialized');

    const entry = this.cache.get(datasetId);
    if (!entry) throw new Error(`Dataset ${datasetId} not cached`);

    // Check TTL
    if (Date.now() - entry.cachedAt > entry.ttlMs) {
      this.removeDataset(datasetId);
      throw new Error(`Dataset ${datasetId} cache expired`);
    }

    let sql = `SELECT * FROM ${entry.tableName}`;
    if (limit !== undefined) sql += ` LIMIT ${limit}`;
    if (offset !== undefined) sql += ` OFFSET ${offset}`;

    return await this.conn.query(sql);
  }

  /**
   * Query cached dataset with an optional SQL WHERE filter.
   * The filter is applied as a WHERE clause on the cached table.
   * Returns an Arrow Table for deck.gl rendering.
   */
  async queryWithFilter(
    datasetId: string,
    whereClause?: string
  ): Promise<Table> {
    await this.init();
    if (!this.conn) throw new Error('DuckDB not initialized');

    const entry = this.cache.get(datasetId);
    if (!entry) throw new Error(`Dataset ${datasetId} not cached`);

    // Check TTL
    if (Date.now() - entry.cachedAt > entry.ttlMs) {
      this.removeDataset(datasetId);
      throw new Error(`Dataset ${datasetId} cache expired`);
    }

    let sql = `SELECT * FROM ${entry.tableName}`;
    if (whereClause && whereClause.trim().length > 0) {
      sql += ` WHERE ${whereClause}`;
    }

    return await this.conn.query(sql);
  }

  /**
   * Get unique values for a column (for categorized styling).
   * Returns up to `limit` unique values sorted.
   */
  async getColumnUniqueValues(
    datasetId: string,
    columnName: string,
    limit = 100
  ): Promise<(string | number)[]> {
    await this.init();
    if (!this.conn) throw new Error('DuckDB not initialized');

    const entry = this.cache.get(datasetId);
    if (!entry) throw new Error(`Dataset ${datasetId} not cached`);

    // Validate column name (basic alphanumeric + underscore check)
    if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(columnName)) {
      throw new Error(`Invalid column name: ${columnName}`);
    }

    const sql = `SELECT DISTINCT "${columnName}" AS val FROM ${entry.tableName} WHERE "${columnName}" IS NOT NULL ORDER BY val LIMIT ${limit}`;
    const result = await this.conn.query(sql);
    const values: (string | number)[] = [];
    for (let i = 0; i < result.numRows; i++) {
      const row = result.get(i);
      if (row) values.push(row.val);
    }
    return values;
  }

  /**
   * Get min/max range for a numeric column (for graduated styling).
   */
  async getColumnRange(
    datasetId: string,
    columnName: string
  ): Promise<{ min: number; max: number } | null> {
    await this.init();
    if (!this.conn) throw new Error('DuckDB not initialized');

    const entry = this.cache.get(datasetId);
    if (!entry) throw new Error(`Dataset ${datasetId} not cached`);

    if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(columnName)) {
      throw new Error(`Invalid column name: ${columnName}`);
    }

    const sql = `SELECT MIN("${columnName}") AS min_val, MAX("${columnName}") AS max_val FROM ${entry.tableName} WHERE "${columnName}" IS NOT NULL`;
    const result = await this.conn.query(sql);
    if (result.numRows === 0) return null;
    const row = result.get(0);
    if (!row || row.min_val === null || row.max_val === null) return null;
    return { min: Number(row.min_val), max: Number(row.max_val) };
  }

  /**
   * Remove a dataset from local cache
   */
  async removeDataset(datasetId: string): Promise<void> {
    await this.init();
    if (!this.conn) return;

    const entry = this.cache.get(datasetId);
    if (entry) {
      try {
        await this.conn.query(`DROP TABLE IF EXISTS ${entry.tableName}`);
      } catch (e) {
        console.warn(`Failed to drop table ${entry.tableName}:`, e);
      }
      this.cache.delete(datasetId);
      await this.saveCacheMetadata();
      await this.deleteArrowBuffer(datasetId);
      console.log(`Removed cached dataset ${datasetId}`);
    }
  }

  /**
   * Clear all cached datasets
   */
  async clearCache(): Promise<void> {
    await this.init();
    
    for (const datasetId of this.cache.keys()) {
      await this.removeDataset(datasetId);
    }
  }

  /**
   * Get all cached dataset IDs
   */
  getCachedDatasets(): CacheEntry[] {
    return Array.from(this.cache.values());
  }

  /**
   * Update TTL for a cached dataset
   */
  async updateTTL(datasetId: string, ttlMs: number): Promise<void> {
    const entry = this.cache.get(datasetId);
    if (entry) {
      entry.ttlMs = ttlMs;
      entry.cachedAt = Date.now(); // Reset timer
      await this.saveCacheMetadata();
    }
  }

  /**
   * Check and clean expired entries
   */
  async cleanExpired(): Promise<string[]> {
    const expired: string[] = [];
    const now = Date.now();

    for (const [id, entry] of this.cache.entries()) {
      if (now - entry.cachedAt > entry.ttlMs) {
        expired.push(id);
      }
    }

    for (const id of expired) {
      await this.removeDataset(id);
    }

    return expired;
  }

  /**
   * Validate that cached tables actually exist in DuckDB.
   * In OPFS mode, tables should persist. In in-memory mode, they're gone.
   */
  private async validateCachedTables(): Promise<void> {
    if (!this.conn) return;

    const staleIds: string[] = [];

    for (const [datasetId, entry] of this.cache.entries()) {
      // Check TTL first
      if (Date.now() - entry.cachedAt > entry.ttlMs) {
        staleIds.push(datasetId);
        continue;
      }
      try {
        await this.conn.query(`SELECT 1 FROM ${entry.tableName} LIMIT 1`);
      } catch (e) {
        staleIds.push(datasetId);
      }
    }

    for (const id of staleIds) {
      this.cache.delete(id);
      await this.deleteArrowBuffer(id);
      console.log(`Cleared stale cache entry for ${id}`);
    }

    if (staleIds.length > 0) {
      await this.saveCacheMetadata();
    }
  }

  /**
   * Re-hydrate tables from IndexedDB Arrow buffers (in-memory fallback).
   * When OPFS is unavailable, tables are lost on refresh but Arrow IPC
   * buffers are still in IndexedDB — reload them without a server round-trip.
   */
  private async rehydrateFromIndexedDB(): Promise<void> {
    if (!this.conn) return;

    const staleIds: string[] = [];
    let rehydrated = 0;

    for (const [datasetId, entry] of this.cache.entries()) {
      // Check TTL
      if (Date.now() - entry.cachedAt > entry.ttlMs) {
        staleIds.push(datasetId);
        continue;
      }

      // Try to reload from IndexedDB Arrow buffer
      const buffer = await this.loadArrowBuffer(datasetId);
      if (!buffer) {
        staleIds.push(datasetId);
        continue;
      }

      try {
        const arrowTable = tableFromIPC(buffer);
        await this.conn.insertArrowTable(arrowTable, { name: entry.tableName, create: true });
        rehydrated++;
        console.log(`Re-hydrated ${datasetId} from IndexedDB (${entry.featureCount} features)`);
      } catch (e) {
        console.warn(`Failed to re-hydrate ${datasetId}:`, e);
        staleIds.push(datasetId);
      }
    }

    // Clean up stale entries
    for (const id of staleIds) {
      this.cache.delete(id);
      await this.deleteArrowBuffer(id);
    }

    if (staleIds.length > 0) {
      await this.saveCacheMetadata();
    }

    if (rehydrated > 0) {
      console.log(`Re-hydrated ${rehydrated} datasets from IndexedDB Arrow buffers`);
    }
  }

  // ============================================================================
  // IndexedDB helpers for cache metadata + Arrow buffer persistence
  // ============================================================================

  private async loadCacheMetadata(): Promise<void> {
    try {
      const db = await this.openIDB();
      const tx = db.transaction(IDB_STORE, 'readonly');
      const store = tx.objectStore(IDB_STORE);
      
      const request = store.getAll();
      const entries: CacheEntry[] = await new Promise((resolve, reject) => {
        request.onsuccess = () => resolve(request.result || []);
        request.onerror = () => reject(request.error);
      });

      for (const entry of entries) {
        this.cache.set(entry.datasetId, entry);
      }

      db.close();
    } catch (e) {
      console.warn('Failed to load cache metadata:', e);
    }
  }

  private async saveCacheMetadata(): Promise<void> {
    try {
      const db = await this.openIDB();
      const tx = db.transaction(IDB_STORE, 'readwrite');
      const store = tx.objectStore(IDB_STORE);

      store.clear();
      for (const entry of this.cache.values()) {
        store.put(entry);
      }

      await new Promise<void>((resolve, reject) => {
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      });

      db.close();
    } catch (e) {
      console.warn('Failed to save cache metadata:', e);
    }
  }

  /** Store raw Arrow IPC buffer in IndexedDB for re-hydration on refresh */
  private async saveArrowBuffer(datasetId: string, buffer: ArrayBuffer): Promise<void> {
    try {
      const db = await this.openIDB();
      const tx = db.transaction(IDB_ARROW_STORE, 'readwrite');
      const store = tx.objectStore(IDB_ARROW_STORE);
      store.put({ datasetId, buffer });

      await new Promise<void>((resolve, reject) => {
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      });
      db.close();
    } catch (e) {
      console.warn('Failed to save Arrow buffer:', e);
    }
  }

  /** Load raw Arrow IPC buffer from IndexedDB */
  private async loadArrowBuffer(datasetId: string): Promise<ArrayBuffer | null> {
    try {
      const db = await this.openIDB();
      const tx = db.transaction(IDB_ARROW_STORE, 'readonly');
      const store = tx.objectStore(IDB_ARROW_STORE);
      const request = store.get(datasetId);

      const result = await new Promise<any>((resolve, reject) => {
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      db.close();
      return result?.buffer || null;
    } catch (e) {
      console.warn('Failed to load Arrow buffer:', e);
      return null;
    }
  }

  /** Delete Arrow IPC buffer from IndexedDB */
  private async deleteArrowBuffer(datasetId: string): Promise<void> {
    try {
      const db = await this.openIDB();
      const tx = db.transaction(IDB_ARROW_STORE, 'readwrite');
      const store = tx.objectStore(IDB_ARROW_STORE);
      store.delete(datasetId);

      await new Promise<void>((resolve, reject) => {
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      });
      db.close();
    } catch (e) {
      console.warn('Failed to delete Arrow buffer:', e);
    }
  }

  private openIDB(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(IDB_NAME, 3);
      
      request.onupgradeneeded = () => {
        const db = request.result;
        // Metadata store
        if (!db.objectStoreNames.contains(IDB_STORE)) {
          db.createObjectStore(IDB_STORE, { keyPath: 'datasetId' });
        }
        // Arrow IPC buffer store (new in v3)
        if (!db.objectStoreNames.contains(IDB_ARROW_STORE)) {
          db.createObjectStore(IDB_ARROW_STORE, { keyPath: 'datasetId' });
        }
      };

      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }
}

// Singleton instance
export const duckdbService = new DuckDBService();
