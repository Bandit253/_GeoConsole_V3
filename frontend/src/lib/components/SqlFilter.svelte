<script lang="ts">
  import type { MapLayer } from '../types/mapStudio';

  interface FieldInfo {
    name: string;
    type: string;
  }

  interface Props {
    layer: MapLayer;
    fields?: FieldInfo[];
    onFilterChange: (sqlFilter: string | undefined) => void;
  }

  let { layer, fields = [], onFilterChange }: Props = $props();

  let filterText = $state(layer.sqlFilter || '');
  let error = $state<string | null>(null);
  let resultCount = $state<number | null>(null);
  let isApplying = $state(false);
  let isExpanded = $state(!!layer.sqlFilter);

  function handleApply() {
    const trimmed = filterText.trim();
    error = null;
    
    if (!trimmed) {
      onFilterChange(undefined);
      resultCount = null;
      return;
    }

    isApplying = true;
    onFilterChange(trimmed);
  }

  function handleClear() {
    filterText = '';
    error = null;
    resultCount = null;
    onFilterChange(undefined);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      handleApply();
    }
  }

  function insertFieldName(fieldName: string) {
    filterText = filterText ? `${filterText} "${fieldName}"` : `"${fieldName}"`;
  }

  function insertSnippet(snippet: string) {
    filterText = filterText ? `${filterText} ${snippet}` : snippet;
  }

  // Sync with external layer changes
  $effect(() => {
    if (layer.sqlFilter !== undefined && layer.sqlFilter !== filterText) {
      filterText = layer.sqlFilter;
    }
  });

  // Update error/result from parent
  export function setResult(count: number) {
    resultCount = count;
    isApplying = false;
    error = null;
  }

  export function setError(msg: string) {
    error = msg;
    isApplying = false;
    resultCount = null;
  }
</script>

<div class="sql-filter">
  <button class="filter-toggle" onclick={() => isExpanded = !isExpanded}>
    <span class="toggle-icon">{isExpanded ? '▼' : '▶'}</span>
    <span>SQL Filter</span>
    {#if layer.sqlFilter}
      <span class="filter-active-badge">Active</span>
    {/if}
  </button>

  {#if isExpanded}
    <div class="filter-content">
      <div class="filter-help">
        <span class="help-label">WHERE</span>
        <span class="help-hint">Ctrl+Enter to apply</span>
      </div>

      <textarea
        class="filter-input"
        class:has-error={!!error}
        placeholder='e.g. "population" > 10000 AND "state" = &#39;NSW&#39;'
        bind:value={filterText}
        onkeydown={handleKeydown}
        rows="3"
      ></textarea>

      {#if fields.length > 0}
        <div class="field-chips">
          {#each fields.slice(0, 12) as field}
            <button
              class="field-chip"
              onclick={() => insertFieldName(field.name)}
              title={`${field.name} (${field.type})`}
            >
              {field.name}
            </button>
          {/each}
          {#if fields.length > 12}
            <span class="more-fields">+{fields.length - 12} more</span>
          {/if}
        </div>
      {/if}

      <div class="snippet-row">
        <button class="snippet-btn" onclick={() => insertSnippet('AND')}>AND</button>
        <button class="snippet-btn" onclick={() => insertSnippet('OR')}>OR</button>
        <button class="snippet-btn" onclick={() => insertSnippet('NOT')}>NOT</button>
        <button class="snippet-btn" onclick={() => insertSnippet('IN ()')}>IN</button>
        <button class="snippet-btn" onclick={() => insertSnippet('LIKE \'%\'')}>LIKE</button>
        <button class="snippet-btn" onclick={() => insertSnippet('IS NOT NULL')}>NOT NULL</button>
        <button class="snippet-btn" onclick={() => insertSnippet('BETWEEN  AND ')}>BETWEEN</button>
      </div>

      <div class="filter-actions">
        <button
          class="btn-primary apply-btn"
          onclick={handleApply}
          disabled={isApplying}
        >
          {isApplying ? 'Applying...' : 'Apply Filter'}
        </button>
        <button
          class="btn-secondary clear-btn"
          onclick={handleClear}
          disabled={!filterText && !layer.sqlFilter}
        >
          Clear
        </button>
      </div>

      {#if error}
        <div class="filter-error">{error}</div>
      {/if}

      {#if resultCount !== null}
        <div class="filter-result">
          Showing {resultCount.toLocaleString()} features
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .sql-filter {
    border-top: 1px solid var(--border);
  }

  .filter-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 10px 16px;
    background: none;
    border: none;
    color: var(--text-primary);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
  }

  .filter-toggle:hover {
    background: var(--bg-tertiary);
  }

  .toggle-icon {
    font-size: 10px;
    color: var(--text-secondary);
  }

  .filter-active-badge {
    margin-left: auto;
    padding: 2px 8px;
    background: var(--accent);
    color: white;
    border-radius: 10px;
    font-size: 10px;
    font-weight: 600;
  }

  .filter-content {
    padding: 0 16px 16px;
  }

  .filter-help {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .help-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
    font-family: monospace;
  }

  .help-hint {
    font-size: 10px;
    color: var(--text-secondary);
  }

  .filter-input {
    width: 100%;
    padding: 8px 10px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-primary);
    font-family: 'Fira Code', 'Cascadia Code', 'Consolas', monospace;
    font-size: 12px;
    line-height: 1.5;
    resize: vertical;
    min-height: 48px;
  }

  .filter-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .filter-input.has-error {
    border-color: var(--danger, #e74c3c);
  }

  .field-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 8px;
  }

  .field-chip {
    padding: 2px 8px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 12px;
    font-size: 10px;
    color: var(--text-secondary);
    cursor: pointer;
    font-family: monospace;
    transition: all 0.15s;
  }

  .field-chip:hover {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .more-fields {
    padding: 2px 8px;
    font-size: 10px;
    color: var(--text-secondary);
  }

  .snippet-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 8px;
  }

  .snippet-btn {
    padding: 2px 6px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 3px;
    font-size: 10px;
    color: var(--text-secondary);
    cursor: pointer;
    font-family: monospace;
  }

  .snippet-btn:hover {
    background: var(--border);
    color: var(--text-primary);
  }

  .filter-actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }

  .apply-btn {
    flex: 1;
    padding: 6px 12px;
    font-size: 12px;
  }

  .clear-btn {
    padding: 6px 12px;
    font-size: 12px;
  }

  .filter-error {
    margin-top: 8px;
    padding: 6px 10px;
    background: rgba(231, 76, 60, 0.15);
    border: 1px solid rgba(231, 76, 60, 0.3);
    border-radius: 4px;
    color: var(--danger, #e74c3c);
    font-size: 11px;
    font-family: monospace;
    word-break: break-word;
  }

  .filter-result {
    margin-top: 8px;
    padding: 4px 10px;
    background: rgba(46, 204, 113, 0.15);
    border-radius: 4px;
    color: #2ecc71;
    font-size: 11px;
  }
</style>
