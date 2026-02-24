<script lang="ts">
  import type { MapLayer, LayerStyle, StyleType } from '../types/mapStudio';
  import { COLOR_RAMPS, DEFAULT_STYLES } from '../types/mapStudio';

  interface FieldInfo {
    name: string;
    type: string; // 'number' | 'string' | 'boolean' etc
  }

  interface Props {
    layer: MapLayer;
    fields?: FieldInfo[];
    onStyleChange: (style: Partial<LayerStyle>) => void;
    onOpacityChange: (opacity: number) => void;
  }

  let { layer, fields = [], onStyleChange, onOpacityChange }: Props = $props();

  let selectedRamp = $state<string>(layer.style.colors ? 
    Object.entries(COLOR_RAMPS).find(([_, colors]) => 
      colors[0] === layer.style.colors?.[0])?.[0] || 'Blues' : 'Blues');

  // Get numeric and string fields for data-driven styling
  const numericFields = $derived(fields.filter(f => 
    ['number', 'int', 'float', 'double', 'integer', 'bigint', 'decimal'].some(t => 
      f.type.toLowerCase().includes(t))));
  
  const stringFields = $derived(fields.filter(f => 
    ['string', 'varchar', 'text', 'char'].some(t => 
      f.type.toLowerCase().includes(t))));

  const allFields = $derived([...numericFields, ...stringFields]);

  function handleColorChange(property: 'fillColor' | 'strokeColor', value: string) {
    onStyleChange({ [property]: value });
  }

  function handleNumberChange(property: keyof LayerStyle, value: number) {
    onStyleChange({ [property]: value });
  }

  function handleStyleTypeChange(type: StyleType) {
    if (type === 'simple') {
      onStyleChange({ type: 'simple', property: undefined, colors: undefined, breaks: undefined, categories: undefined });
    } else {
      onStyleChange({ type, colors: COLOR_RAMPS[selectedRamp] });
    }
  }

  function handleFieldChange(field: string) {
    const fieldInfo = fields.find(f => f.name === field);
    const isNumeric = fieldInfo && numericFields.some(f => f.name === field);
    
    onStyleChange({ 
      property: field,
      type: isNumeric ? 'graduated' : 'categorized',
      colors: COLOR_RAMPS[selectedRamp]
    });
  }

  function handleRampChange(rampName: string) {
    selectedRamp = rampName;
    if (layer.style.type !== 'simple') {
      onStyleChange({ colors: COLOR_RAMPS[rampName] });
    }
  }

  function resetToDefault() {
    const geomType = layer.geometryType || 'Polygon';
    const defaultStyle = DEFAULT_STYLES[geomType];
    if (defaultStyle) {
      onStyleChange(defaultStyle);
    }
  }

  function isPointLayer(): boolean {
    return layer.geometryType?.includes('Point') ?? false;
  }

  function isLineLayer(): boolean {
    return layer.geometryType?.includes('Line') ?? false;
  }

  function isPolygonLayer(): boolean {
    return layer.geometryType?.includes('Polygon') ?? true;
  }
</script>

<div class="style-editor">
  <div class="panel-header">
    <h2>Style: {layer.name}</h2>
  </div>

  <div class="style-content">
    <!-- Opacity -->
    <div class="style-section">
      <label>
        Layer Opacity
        <span class="value">{Math.round(layer.opacity * 100)}%</span>
      </label>
      <input
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={layer.opacity}
        oninput={(e) => onOpacityChange(parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>

    <!-- Style Type Selector -->
    {#if allFields.length > 0}
      <div class="style-section">
        <label>Style Type</label>
        <div class="style-type-selector">
          <button 
            class="type-btn" 
            class:active={layer.style.type === 'simple' || !layer.style.type}
            onclick={() => handleStyleTypeChange('simple')}
          >
            Simple
          </button>
          <button 
            class="type-btn" 
            class:active={layer.style.type === 'graduated' || layer.style.type === 'categorized'}
            onclick={() => handleStyleTypeChange('graduated')}
          >
            By Attribute
          </button>
        </div>
      </div>
    {/if}

    <!-- Field Selection (for data-driven styling) -->
    {#if (layer.style.type === 'graduated' || layer.style.type === 'categorized') && allFields.length > 0}
      <div class="style-section">
        <label>Color by Field</label>
        <select 
          class="field-select"
          value={layer.style.property || ''}
          onchange={(e) => handleFieldChange((e.target as HTMLSelectElement).value)}
        >
          <option value="">Select field...</option>
          {#if numericFields.length > 0}
            <optgroup label="Numeric (Graduated)">
              {#each numericFields as field}
                <option value={field.name}>{field.name}</option>
              {/each}
            </optgroup>
          {/if}
          {#if stringFields.length > 0}
            <optgroup label="Text (Categorized)">
              {#each stringFields as field}
                <option value={field.name}>{field.name}</option>
              {/each}
            </optgroup>
          {/if}
        </select>
      </div>

      <!-- Color Ramp Selection -->
      <div class="style-section">
        <label>Color Ramp</label>
        <div class="color-ramps compact">
          {#each Object.entries(COLOR_RAMPS) as [name, colors]}
            <button
              class="ramp-btn"
              class:selected={selectedRamp === name}
              onclick={() => handleRampChange(name)}
              title={name}
            >
              <div class="ramp-preview">
                {#each colors as color}
                  <span style="background: {color}"></span>
                {/each}
              </div>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Fill Color (Points and Polygons) - Only show for simple style -->
    {#if !isLineLayer() && (layer.style.type === 'simple' || !layer.style.type)}
      <div class="style-section">
        <label>Fill Color</label>
        <div class="color-input">
          <input
            type="color"
            value={layer.style.fillColor as string || '#2ecc71'}
            oninput={(e) => handleColorChange('fillColor', (e.target as HTMLInputElement).value)}
          />
          <input
            type="text"
            value={layer.style.fillColor as string || '#2ecc71'}
            oninput={(e) => handleColorChange('fillColor', (e.target as HTMLInputElement).value)}
          />
        </div>
      </div>
    {/if}

    <!-- Stroke Color -->
    <div class="style-section">
      <label>Stroke Color</label>
      <div class="color-input">
        <input
          type="color"
          value={layer.style.strokeColor as string || '#27ae60'}
          oninput={(e) => handleColorChange('strokeColor', (e.target as HTMLInputElement).value)}
        />
        <input
          type="text"
          value={layer.style.strokeColor as string || '#27ae60'}
          oninput={(e) => handleColorChange('strokeColor', (e.target as HTMLInputElement).value)}
        />
      </div>
    </div>

    <!-- Stroke Width -->
    <div class="style-section">
      <label>
        Stroke Width
        <span class="value">{layer.style.strokeWidth || 1}px</span>
      </label>
      <input
        type="range"
        min="0"
        max="20"
        step="0.5"
        value={layer.style.strokeWidth || 1}
        oninput={(e) => handleNumberChange('strokeWidth', parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>

    <!-- Point Radius (Points only) -->
    {#if isPointLayer()}
      <div class="style-section">
        <label>
          Point Radius
          <span class="value">{layer.style.radius || 5}px</span>
        </label>
        <input
          type="range"
          min="1"
          max="50"
          step="1"
          value={layer.style.radius || 5}
          oninput={(e) => handleNumberChange('radius', parseFloat((e.target as HTMLInputElement).value))}
        />
      </div>
    {/if}

    <!-- Fill Opacity -->
    <div class="style-section">
      <label>
        Fill Opacity
        <span class="value">{Math.round((layer.style.opacity || 0.6) * 100)}%</span>
      </label>
      <input
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={layer.style.opacity || 0.6}
        oninput={(e) => handleNumberChange('opacity', parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>

    
    <!-- Reset Button -->
    <div class="style-section">
      <button class="btn-secondary reset-btn" onclick={resetToDefault}>
        Reset to Default
      </button>
    </div>
  </div>
</div>

<style>
  .style-editor {
    display: flex;
    flex-direction: column;
  }

  .panel-header {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .panel-header h2 {
    font-size: 14px;
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .style-content {
    padding: 16px;
  }

  .style-section {
    margin-bottom: 20px;
  }

  .style-section label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 8px;
  }

  .value {
    color: var(--text-primary);
    font-weight: 500;
  }

  input[type="range"] {
    width: 100%;
    height: 6px;
    -webkit-appearance: none;
    background: var(--bg-tertiary);
    border-radius: 3px;
    outline: none;
  }

  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    background: var(--accent);
    border-radius: 50%;
    cursor: pointer;
  }

  .color-input {
    display: flex;
    gap: 8px;
  }

  .color-input input[type="color"] {
    width: 40px;
    height: 32px;
    border: none;
    padding: 0;
    cursor: pointer;
  }

  .color-input input[type="text"] {
    flex: 1;
    font-family: monospace;
  }

  .color-ramps {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .ramp-btn {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .ramp-btn:hover {
    background: var(--border);
  }

  .ramp-btn.selected {
    border-color: var(--accent);
    background: rgba(74, 144, 226, 0.1);
  }

  .ramp-preview {
    display: flex;
    flex: 1;
    height: 16px;
    border-radius: 2px;
    overflow: hidden;
  }

  .ramp-preview span {
    flex: 1;
  }

  .ramp-name {
    font-size: 11px;
    color: var(--text-secondary);
    width: 60px;
    text-align: right;
  }

  .reset-btn {
    width: 100%;
  }

  .style-type-selector {
    display: flex;
    gap: 4px;
  }

  .type-btn {
    flex: 1;
    padding: 8px 12px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 12px;
    color: white;
    cursor: pointer;
    transition: all 0.15s;
  }

  .type-btn:hover {
    background: var(--border);
  }

  .type-btn.active {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }

  .field-select {
    width: 100%;
    padding: 8px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 12px;
    color: var(--text-primary);
  }

  .color-ramps.compact {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px;
  }

  .color-ramps.compact .ramp-btn {
    padding: 4px;
  }

  .color-ramps.compact .ramp-preview {
    height: 12px;
  }
</style>
