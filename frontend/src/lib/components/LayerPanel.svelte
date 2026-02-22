<script lang="ts">
  import type { MapLayer } from '../types/mapStudio';

  interface Props {
    layers: MapLayer[];
    selectedLayerId: string | null;
    onSelect: (id: string) => void;
    onVisibilityChange: (id: string, visible: boolean) => void;
    onRemove: (id: string) => void;
    onMove: (id: string, direction: 'up' | 'down') => void;
  }

  let { layers, selectedLayerId, onSelect, onVisibilityChange, onRemove, onMove }: Props = $props();

  function getGeometryIcon(type: string | undefined): string {
    if (!type) return '📍';
    if (type.includes('Point')) return '📍';
    if (type.includes('Line')) return '📏';
    return '⬛';
  }

  // Reverse order so top layer is at top of list
  let reversedLayers = $derived([...layers].reverse());
</script>

<div class="layer-panel">
  <div class="panel-header">
    <h2>Layers</h2>
    <span class="layer-count">{layers.length}</span>
  </div>

  {#if layers.length === 0}
    <div class="empty-state">
      <p>No layers yet.</p>
      <p class="hint">Click "Add Layer" to get started.</p>
    </div>
  {:else}
    <ul class="layer-list">
      {#each reversedLayers as layer (layer.id)}
        <li
          class="layer-item"
          class:selected={layer.id === selectedLayerId}
          onclick={() => onSelect(layer.id)}
        >
          <button
            class="visibility-btn"
            onclick={(e) => { e.stopPropagation(); onVisibilityChange(layer.id, !layer.visible); }}
            title={layer.visible ? 'Hide layer' : 'Show layer'}
          >
            {layer.visible ? '👁️' : '👁️‍🗨️'}
          </button>

          <span class="layer-icon">{getGeometryIcon(layer.geometryType)}</span>
          
          <div class="layer-info">
            <span class="layer-name">{layer.name}</span>
            {#if layer.featureCount !== undefined}
              <span class="feature-count">{layer.featureCount} features</span>
            {/if}
          </div>

          <div class="layer-actions">
            <button
              class="action-btn"
              onclick={(e) => { e.stopPropagation(); onMove(layer.id, 'up'); }}
              disabled={layer.zIndex === layers.length - 1}
              title="Move up"
            >
              ⬆️
            </button>
            <button
              class="action-btn"
              onclick={(e) => { e.stopPropagation(); onMove(layer.id, 'down'); }}
              disabled={layer.zIndex === 0}
              title="Move down"
            >
              ⬇️
            </button>
            <button
              class="action-btn delete-btn"
              onclick={(e) => { e.stopPropagation(); if (confirm('Delete this layer?')) onRemove(layer.id); }}
              title="Delete layer"
            >
              🗑️
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .layer-panel {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .panel-header h2 {
    font-size: 14px;
    margin: 0;
  }

  .layer-count {
    background: var(--bg-tertiary);
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 12px;
  }

  .empty-state {
    padding: 24px 16px;
    text-align: center;
    color: var(--text-secondary);
  }

  .hint {
    font-size: 12px;
    margin-top: 8px;
  }

  .layer-list {
    list-style: none;
    padding: 0;
    margin: 0;
    flex: 1;
    overflow-y: auto;
  }

  .layer-item {
    display: flex;
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    gap: 8px;
    transition: background 0.15s;
  }

  .layer-item:hover {
    background: var(--bg-tertiary);
  }

  .layer-item.selected {
    background: var(--accent);
    background: rgba(74, 144, 226, 0.2);
    border-left: 3px solid var(--accent);
  }

  .visibility-btn {
    background: none;
    border: none;
    padding: 4px;
    font-size: 14px;
    opacity: 0.7;
  }

  .visibility-btn:hover {
    opacity: 1;
  }

  .layer-icon {
    font-size: 16px;
  }

  .layer-info {
    flex: 1;
    min-width: 0;
  }

  .layer-name {
    display: block;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .feature-count {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .layer-actions {
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .layer-item:hover .layer-actions {
    opacity: 1;
  }

  .action-btn {
    background: none;
    border: none;
    padding: 4px;
    font-size: 12px;
    opacity: 0.7;
  }

  .action-btn:hover:not(:disabled) {
    opacity: 1;
  }

  .action-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .delete-btn:hover {
    color: var(--danger);
  }
</style>
