<script lang="ts">
  import { BASEMAPS } from '../types/mapStudio';

  interface Props {
    currentBasemap: string;
    onSelect: (basemapId: string) => void;
  }

  let { currentBasemap, onSelect }: Props = $props();

  let isOpen = $state(false);

  function handleSelect(basemapId: string) {
    onSelect(basemapId);
    isOpen = false;
  }

  function getCurrentLabel(): string {
    return BASEMAPS.find(b => b.id === currentBasemap)?.label || 'Select Basemap';
  }
</script>

<div class="basemap-selector">
  <button class="selector-btn" onclick={() => isOpen = !isOpen}>
    🗺️ {getCurrentLabel()}
    <span class="arrow">{isOpen ? '▲' : '▼'}</span>
  </button>

  {#if isOpen}
    <div class="dropdown" onmouseleave={() => isOpen = false}>
      <div class="basemap-grid">
        {#each BASEMAPS as basemap}
          <button
            class="basemap-option"
            class:selected={basemap.id === currentBasemap}
            onclick={() => handleSelect(basemap.id)}
          >
            <div class="basemap-preview" style="background: {getPreviewColor(basemap.id)}">
              {#if basemap.id === currentBasemap}
                <span class="checkmark">✓</span>
              {/if}
            </div>
            <span class="basemap-label">{basemap.label}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<script context="module" lang="ts">
  function getPreviewColor(id: string): string {
    switch (id) {
      case 'osm': return '#d4e6f1';
      case 'osm-light': return '#f5f5f5';
      case 'osm-dark': return '#2c3e50';
      case 'satellite': return '#1a3a1a';
      case 'terrain': return '#c4a05a';
      case 'none': return '#666';
      default: return '#ccc';
    }
  }
</script>

<style>
  .basemap-selector {
    position: relative;
  }

  .selector-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background-color: #0f0f23;
    border: 1px solid #2a2a4a;
    color: #eaeaea;
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .selector-btn:hover {
    background-color: #2a2a4a;
  }

  .arrow {
    font-size: 10px;
    opacity: 0.7;
  }

  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    margin-top: 4px;
    background-color: #16213e;
    border: 1px solid #2a2a4a;
    border-radius: 4px;
    padding: 12px;
    z-index: 100;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .basemap-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
    width: 240px;
  }

  .basemap-option {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px;
    background-color: #0f0f23;
    border: 2px solid transparent;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .basemap-option:hover {
    background-color: #2a2a4a;
  }

  .basemap-option.selected {
    border-color: #4a90e2;
  }

  .basemap-preview {
    width: 100%;
    height: 40px;
    border-radius: 2px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .checkmark {
    color: white;
    font-weight: bold;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
  }

  .basemap-label {
    font-size: 11px;
    color: #a0a0a0;
  }
</style>
