<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import maplibregl from 'maplibre-gl';
  import { mapStudioStore } from '../stores/mapStudio.svelte';
  import { BASEMAPS, type MapLayer } from '../types/mapStudio';
  import { listDatasets, getArrowTableForDeckGL, uploadDataset, isDatasetCached, removeFromCache } from '../services/api';
  import { deckglService } from '../services/deckgl';
  import LayerPanel from './LayerPanel.svelte';
  import StyleEditor from './StyleEditor.svelte';
  import BasemapSelector from './BasemapSelector.svelte';

  let mapContainer: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let fileInput: HTMLInputElement;

  // Reactive state
  let showAddLayerDialog = $state(false);
  let uploading = $state(false);
  let error = $state<string | null>(null);
  let previousBbox: [number, number, number, number] | null = null;
  let moveEndTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(async () => {
    initMap();
    await loadDatasets();
  });

  onDestroy(() => {
    deckglService.dispose();
    map?.remove();
  });

  function initMap() {
    const basemap = mapStudioStore.currentBasemap;
    
    map = new maplibregl.Map({
      container: mapContainer,
      style: {
        version: 8,
        sources: {
          'basemap': {
            type: 'raster',
            tiles: basemap.url ? [basemap.url] : [],
            tileSize: 256,
            attribution: basemap.attribution,
            maxzoom: basemap.maxzoom || 19
          }
        },
        layers: basemap.url ? [{
          id: 'basemap-layer',
          type: 'raster',
          source: 'basemap'
        }] : []
      },
      center: mapStudioStore.state.view.center,
      zoom: mapStudioStore.state.view.zoom,
      bearing: mapStudioStore.state.view.bearing,
      pitch: mapStudioStore.state.view.pitch
    });

    map.addControl(new maplibregl.NavigationControl(), 'top-right');
    map.addControl(new maplibregl.ScaleControl(), 'bottom-right');

    map.on('moveend', () => {
      if (!map) return;
      mapStudioStore.setView({
        center: [map.getCenter().lng, map.getCenter().lat],
        zoom: map.getZoom(),
        bearing: map.getBearing(),
        pitch: map.getPitch()
      });

      // Debounced viewport reload
      if (moveEndTimer) clearTimeout(moveEndTimer);
      moveEndTimer = setTimeout(() => reloadVisibleLayers(), 300);
    });
    
    // Initialize deck.gl overlay for Arrow rendering
    map.on('load', () => {
      if (map) {
        deckglService.init(map);
        console.log('deck.gl overlay initialized on map load');
      }
    });
  }

  async function loadDatasets() {
    try {
      const datasets = await listDatasets();
      mapStudioStore.setDatasets(datasets);
    } catch (e) {
      console.error('Failed to load datasets:', e);
    }
  }

  async function handleFileUpload(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    uploading = true;
    error = null;

    try {
      const dataset = await uploadDataset(file);
      mapStudioStore.setDatasets([...mapStudioStore.datasets, dataset]);
      
      // Automatically add as layer
      const layer = mapStudioStore.addLayerFromDataset(dataset);
      await loadLayerData(layer);
      
      showAddLayerDialog = false;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Upload failed';
    } finally {
      uploading = false;
      input.value = '';
    }
  }

  function getMapBbox(): [number, number, number, number] | undefined {
    if (!map) return undefined;
    const bounds = map.getBounds();
    return [bounds.getWest(), bounds.getSouth(), bounds.getEast(), bounds.getNorth()];
  }

  function bboxChangedSignificantly(newBbox: [number, number, number, number]): boolean {
    if (!previousBbox) return true;
    // Check if any edge moved by more than 10% of the viewport extent
    const wThresh = (previousBbox[2] - previousBbox[0]) * 0.1;
    const hThresh = (previousBbox[3] - previousBbox[1]) * 0.1;
    return (
      Math.abs(newBbox[0] - previousBbox[0]) > wThresh ||
      Math.abs(newBbox[1] - previousBbox[1]) > hThresh ||
      Math.abs(newBbox[2] - previousBbox[2]) > wThresh ||
      Math.abs(newBbox[3] - previousBbox[3]) > hThresh
    );
  }

  // Threshold for server-side bbox filtering (features).
  // Below this we load the full dataset once and let deck.gl clip on the GPU.
  const BBOX_FILTER_THRESHOLD = 500_000;

  async function reloadVisibleLayers() {
    const bbox = getMapBbox();
    if (!bbox || !bboxChangedSignificantly(bbox)) return;
    previousBbox = bbox;

    for (const layer of mapStudioStore.state.layers) {
      if (layer.visible && layer.datasetId) {
        // Skip reload if already cached — deck.gl handles viewport clipping
        if (isDatasetCached(layer.datasetId)) continue;
        await loadLayerData(layer, bbox);
      }
    }
  }

  async function loadLayerData(layer: MapLayer, bbox?: [number, number, number, number]) {
    if (!layer.datasetId || !map) return;

    try {
      console.log(`Loading layer ${layer.name} via Arrow IPC...`);
      
      // Only use server-side bbox filtering for very large datasets
      const dataset = mapStudioStore.datasets.find(d => d.id === layer.datasetId);
      const useServerBbox = bbox && dataset && dataset.feature_count > BBOX_FILTER_THRESHOLD;

      const arrowTable = await getArrowTableForDeckGL(
        layer.datasetId,
        layer.geometryType || 'Polygon',
        undefined,
        useServerBbox ? bbox : undefined
      );
      
      console.log(`Loaded Arrow table: ${arrowTable.numRows} rows for layer ${layer.name}`);
      
      // Add to deck.gl overlay
      const style = {
        fillColor: hexToRgba(layer.style.fillColor as string || '#3388ff', layer.opacity),
        strokeColor: hexToRgba(layer.style.strokeColor as string || '#2171b5', 1),
        strokeWidth: layer.style.strokeWidth || 1,
        radius: layer.style.radius || 5,
        opacity: layer.opacity,
      };
      
      deckglService.addArrowLayer(
        layer.id,
        arrowTable,
        layer.geometryType || 'Polygon',
        style,
        layer.visible
      );
      
      // Zoom to dataset bounds (from backend metadata — no WKB parsing needed)
      zoomToDatasetBounds(layer.datasetId);
    } catch (e) {
      console.error('Failed to load layer data:', e);
    }
  }
  
  function hexToRgba(hex: string, alpha: number): [number, number, number, number] {
    const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
    if (result) {
      return [
        parseInt(result[1], 16),
        parseInt(result[2], 16),
        parseInt(result[3], 16),
        Math.round(alpha * 255)
      ];
    }
    return [100, 100, 100, Math.round(alpha * 255)];
  }
  
  function zoomToDatasetBounds(datasetId: string) {
    if (!map) return;
    const dataset = mapStudioStore.datasets.find(d => d.id === datasetId);
    if (!dataset?.bounds) return;
    const { min_x, min_y, max_x, max_y } = dataset.bounds;
    if (!isFinite(min_x) || !isFinite(min_y) || !isFinite(max_x) || !isFinite(max_y)) return;
    map.fitBounds([[min_x, min_y], [max_x, max_y]], { padding: 50, maxZoom: 18 });
  }

  // addLayerToMap (legacy MapLibre GeoJSON path) removed.
  // All rendering now goes through deck.gl GeoArrow layers for zero-copy Arrow → GPU.

  function handleBasemapChange(basemapId: string) {
    mapStudioStore.setBasemap(basemapId);
    const basemap = BASEMAPS.find(b => b.id === basemapId);
    if (!map || !basemap) return;

    const source = map.getSource('basemap') as maplibregl.RasterTileSource;
    if (source && basemap.url) {
      source.setTiles([basemap.url]);
    }
  }

  function handleLayerVisibilityChange(layerId: string, visible: boolean) {
    mapStudioStore.setLayerVisibility(layerId, visible);
    // Update deck.gl layer visibility
    deckglService.setLayerVisibility(layerId, visible);
  }

  function handleExport() {
    const state = mapStudioStore.exportState();
    const json = JSON.stringify(state, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    
    const a = document.createElement('a');
    a.href = url;
    a.download = `${state.name.replace(/\s+/g, '_')}.json`;
    a.click();
    
    URL.revokeObjectURL(url);
  }

  function applyStyleToMap(layerId: string, partialStyle: any) {
    const layer = mapStudioStore.state.layers.find(l => l.id === layerId);
    if (!layer) return;
    
    // Merge partial style with current layer style to get full style
    const fullStyle = { ...layer.style, ...partialStyle };
    
    console.log('applyStyleToMap:', {
      partialStyle,
      layerStyle: layer.style,
      fullStyle,
      styleType: fullStyle.type,
      property: fullStyle.property,
      colorsLength: fullStyle.colors?.length
    });
    
    // Update deck.gl layer style including data-driven properties
    deckglService.updateLayerStyle(layerId, {
      fillColor: fullStyle.fillColor ? hexToRgba(fullStyle.fillColor, layer.opacity) : undefined,
      strokeColor: fullStyle.strokeColor ? hexToRgba(fullStyle.strokeColor, 1) : undefined,
      strokeWidth: fullStyle.strokeWidth,
      radius: fullStyle.radius,
      opacity: fullStyle.opacity ?? layer.opacity,
      // Data-driven styling
      styleType: fullStyle.type,
      property: fullStyle.property,
      colors: fullStyle.colors,
      breaks: fullStyle.breaks,
      categories: fullStyle.categories,
    });
  }

  function applyOpacityToMap(layerId: string, opacity: number) {
    const layer = mapStudioStore.state.layers.find(l => l.id === layerId);
    if (!layer) return;
    
    // Update deck.gl layer opacity
    deckglService.updateLayerStyle(layerId, {
      opacity: opacity,
    });
  }

  function handleLayerMove(layerId: string, direction: 'up' | 'down') {
    // Update store first
    mapStudioStore.moveLayer(layerId, direction);
    
    // Sync deck.gl layer order to match store order
    const layers = mapStudioStore.state.layers;
    const layerIds = layers.map(l => l.id);
    deckglService.reorderLayers(layerIds);
  }
</script>

<div class="map-studio">
  <!-- Toolbar -->
  <header class="toolbar">
    <div class="toolbar-left">
      <h1>🗺️ Map Studio</h1>
      <input
        type="text"
        class="map-name"
        value={mapStudioStore.state.name}
        oninput={(e) => mapStudioStore.setMapName((e.target as HTMLInputElement).value)}
      />
    </div>

    <div class="toolbar-center">
      <BasemapSelector
        currentBasemap={mapStudioStore.state.basemap}
        onSelect={handleBasemapChange}
      />
      
      <button class="btn-primary" onclick={() => showAddLayerDialog = true}>
        ➕ Add Layer
      </button>
    </div>

    <div class="toolbar-right">
      <button class="btn-secondary" onclick={() => mapStudioStore.toggleLayerPanel()}>
        📑 Layers
      </button>
      <button class="btn-secondary" onclick={() => mapStudioStore.toggleStyleEditor()}>
        🎨 Style
      </button>
      <button class="btn-secondary" onclick={handleExport}>
        💾 Export
      </button>
    </div>
  </header>

  <div class="main-content">
    <!-- Layer Panel -->
    {#if mapStudioStore.state.showLayerPanel}
      <aside class="panel layer-panel">
        <LayerPanel
          layers={mapStudioStore.state.layers}
          selectedLayerId={mapStudioStore.state.selectedLayerId}
          onSelect={(id) => mapStudioStore.selectLayer(id)}
          onVisibilityChange={handleLayerVisibilityChange}
          onRemove={(id) => mapStudioStore.removeLayer(id)}
          onMove={handleLayerMove}
        />
      </aside>
    {/if}

    <!-- Map Container -->
    <div class="map-container" bind:this={mapContainer}></div>

    <!-- Style Editor -->
    {#if mapStudioStore.state.showStyleEditor && mapStudioStore.selectedLayer}
      {@const selectedDataset = mapStudioStore.datasets.find(d => d.id === mapStudioStore.selectedLayer?.datasetId)}
      {@const layerFields = selectedDataset?.columns
        .filter(c => c.name !== 'geometry' && c.name !== '__geometry')
        .map(c => ({ name: c.name, type: c.data_type })) || []}
      <aside class="panel style-panel">
        <StyleEditor
          layer={mapStudioStore.selectedLayer}
          fields={layerFields}
          onStyleChange={(style) => {
            if (mapStudioStore.state.selectedLayerId) {
              mapStudioStore.updateLayerStyle(mapStudioStore.state.selectedLayerId, style);
              applyStyleToMap(mapStudioStore.state.selectedLayerId, style);
            }
          }}
          onOpacityChange={(opacity) => {
            if (mapStudioStore.state.selectedLayerId) {
              mapStudioStore.setLayerOpacity(mapStudioStore.state.selectedLayerId, opacity);
              applyOpacityToMap(mapStudioStore.state.selectedLayerId, opacity);
            }
          }}
        />
      </aside>
    {/if}
  </div>

  <!-- Map Info Overlay -->
  <div class="map-info">
    <span>Layers: {mapStudioStore.state.layers.length}</span>
    <span>Zoom: {mapStudioStore.state.view.zoom.toFixed(1)}</span>
    <span>Center: {mapStudioStore.state.view.center[0].toFixed(4)}, {mapStudioStore.state.view.center[1].toFixed(4)}</span>
  </div>

  <!-- Add Layer Dialog -->
  {#if showAddLayerDialog}
    <div class="dialog-overlay" onclick={() => showAddLayerDialog = false}>
      <div class="dialog" onclick={(e) => e.stopPropagation()}>
        <h2>Add Layer</h2>
        
        <div class="dialog-content">
          <h3>Upload Spatial File</h3>
          <p>Supported formats: GeoParquet, GeoPackage, Shapefile, GeoJSON, KML</p>
          
          <input
            bind:this={fileInput}
            type="file"
            accept=".parquet,.geoparquet,.gpkg,.shp,.geojson,.json,.kml"
            onchange={handleFileUpload}
            disabled={uploading}
          />

          {#if uploading}
            <p class="status">Uploading and processing...</p>
          {/if}

          {#if error}
            <p class="error">{error}</p>
          {/if}

          {#if mapStudioStore.datasets.length > 0}
            <h3>Or Select Existing Dataset</h3>
            <ul class="dataset-list">
              {#each mapStudioStore.datasets as dataset}
                <li>
                  <button
                    class="btn-secondary"
                    onclick={async () => {
                      const layer = mapStudioStore.addLayerFromDataset(dataset);
                      await loadLayerData(layer);
                      showAddLayerDialog = false;
                    }}
                  >
                    {dataset.name} ({dataset.feature_count} features)
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        <div class="dialog-actions">
          <button class="btn-secondary" onclick={() => showAddLayerDialog = false}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .map-studio {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background-color: #1a1a2e;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    background-color: #16213e;
    border-bottom: 1px solid #2a2a4a;
    gap: 16px;
  }

  .toolbar-left, .toolbar-center, .toolbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .toolbar h1 {
    font-size: 18px;
    margin: 0;
  }

  .map-name {
    background: transparent;
    border: 1px solid transparent;
    font-size: 16px;
    padding: 4px 8px;
  }

  .map-name:hover, .map-name:focus {
    border-color: var(--border);
  }

  .main-content {
    flex: 1;
    display: flex;
    position: relative;
    overflow: hidden;
  }

  .panel {
    width: 320px;
    background-color: #16213e;
    border: 1px solid #2a2a4a;
    overflow-y: auto;
  }

  .layer-panel {
    border-right: 1px solid #2a2a4a;
  }

  .style-panel {
    border-left: 1px solid #2a2a4a;
  }

  .map-container {
    flex: 1;
    min-width: 0;
  }

  .map-info {
    position: absolute;
    bottom: 8px;
    left: 50%;
    transform: translateX(-50%);
    background-color: #16213e;
    padding: 4px 12px;
    border-radius: 4px;
    font-size: 12px;
    display: flex;
    gap: 16px;
    opacity: 0.9;
  }

  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background-color: #16213e;
    border-radius: 8px;
    padding: 24px;
    min-width: 400px;
    max-width: 600px;
  }

  .dialog h2 {
    margin: 0 0 16px 0;
  }

  .dialog h3 {
    margin: 16px 0 8px 0;
    font-size: 14px;
    color: var(--text-secondary);
  }

  .dialog-content {
    margin-bottom: 16px;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .dataset-list {
    list-style: none;
    padding: 0;
    margin: 8px 0;
  }

  .dataset-list li {
    margin-bottom: 8px;
  }

  .dataset-list button {
    width: 100%;
    text-align: left;
  }

  .status {
    color: var(--accent);
  }

  .error {
    color: var(--danger);
  }
</style>
