<script lang="ts">
  import type { Dataset } from '../types/mapStudio';
  import { previewSql, createDatasetFromSql, type SqlPreviewResponse } from '../services/api';

  interface Props {
    datasets: Dataset[];
    onClose: () => void;
    onDatasetCreated: (dataset: Dataset) => void;
  }

  let { datasets, onClose, onDatasetCreated }: Props = $props();

  let sql = $state('');
  let datasetName = $state('');
  let preview = $state<SqlPreviewResponse | null>(null);
  let error = $state<string | null>(null);
  let running = $state(false);
  let saving = $state(false);

  const SPATIAL_SNIPPETS = [
    { label: 'ST_Buffer', text: 'ST_Buffer(geometry, distance)' },
    { label: 'ST_Intersects', text: 'ST_Intersects(a.geometry, b.geometry)' },
    { label: 'ST_Distance', text: 'ST_Distance(a.geometry, b.geometry)' },
    { label: 'ST_Within', text: 'ST_Within(a.geometry, b.geometry)' },
    { label: 'ST_Union', text: 'ST_Union(geometry)' },
    { label: 'ST_Centroid', text: 'ST_Centroid(geometry)' },
    { label: 'ST_Envelope', text: 'ST_Envelope(geometry)' },
    { label: 'ST_Area', text: 'ST_Area(geometry)' },
    { label: 'ST_Length', text: 'ST_Length(geometry)' },
    { label: 'ST_AsText', text: 'ST_AsText(geometry)' },
    { label: 'ST_Transform', text: "ST_Transform(geometry, 'EPSG:4326', 'EPSG:4326')" },
    { label: 'ST_Difference', text: 'ST_Difference(a.geometry, b.geometry)' },
  ];

  const SQL_SNIPPETS = [
    { label: 'SELECT *', text: 'SELECT * FROM ' },
    { label: 'WHERE', text: ' WHERE ' },
    { label: 'JOIN', text: ' JOIN  ON ' },
    { label: 'GROUP BY', text: ' GROUP BY ' },
    { label: 'ORDER BY', text: ' ORDER BY ' },
    { label: 'LIMIT', text: ' LIMIT 1000' },
    { label: 'DISTINCT', text: 'SELECT DISTINCT ' },
    { label: 'COUNT', text: 'COUNT(*)' },
  ];

  function insertText(text: string) {
    sql = sql ? sql + text : text;
    sqlTextarea?.focus();
  }

  function insertTableRef(tableName: string) {
    insertText(tableName);
  }

  let sqlTextarea: HTMLTextAreaElement | null = null;

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handlePreview();
    }
  }

  async function handlePreview() {
    if (!sql.trim()) return;
    running = true;
    error = null;
    preview = null;
    try {
      preview = await previewSql(sql);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Query failed';
    } finally {
      running = false;
    }
  }

  async function handleSave() {
    if (!sql.trim() || !datasetName.trim()) return;
    saving = true;
    error = null;
    try {
      const dataset = await createDatasetFromSql(sql, datasetName.trim());
      onDatasetCreated(dataset);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Save failed';
      saving = false;
    }
  }
</script>

<div class="sql-console-overlay" onclick={onClose} role="dialog" aria-modal="true">
  <div class="sql-console" onclick={(e) => e.stopPropagation()}>
    <!-- Header -->
    <div class="console-header">
      <h2>SQL Console</h2>
      <span class="header-hint">Full DuckDB SQL — standard + spatial (ST_) functions</span>
      <button class="close-btn" onclick={onClose} aria-label="Close">✕</button>
    </div>

    <div class="console-body">
      <!-- Left: editor -->
      <div class="editor-pane">
        <!-- Available datasets -->
        <div class="tables-section">
          <span class="section-label">Available Tables</span>
          <div class="table-chips">
            {#each datasets as ds}
              <button
                class="table-chip"
                onclick={() => insertTableRef(ds.table_name)}
                title="{ds.name} ({ds.feature_count} rows) — click to insert table name"
              >
                <span class="chip-name">{ds.name}</span>
                <span class="chip-table">{ds.table_name}</span>
              </button>
            {/each}
            {#if datasets.length === 0}
              <span class="no-tables">No datasets loaded yet</span>
            {/if}
          </div>
        </div>

        <!-- SQL snippets -->
        <div class="snippets-row">
          {#each SQL_SNIPPETS as s}
            <button class="snippet-btn" onclick={() => insertText(s.text)}>{s.label}</button>
          {/each}
        </div>
        <div class="snippets-row">
          {#each SPATIAL_SNIPPETS as s}
            <button class="snippet-btn spatial" onclick={() => insertText(s.text)}>{s.label}</button>
          {/each}
        </div>

        <!-- SQL Editor -->
        <textarea
          bind:this={sqlTextarea}
          class="sql-editor"
          class:has-error={!!error}
          bind:value={sql}
          onkeydown={handleKeydown}
          placeholder="SELECT * FROM dataset_xxx&#10;WHERE ST_Intersects(geometry, ST_Buffer(ST_Point(151.2, -33.8), 0.1))&#10;&#10;-- Ctrl+Enter to preview"
          spellcheck="false"
          autocomplete="off"
        ></textarea>

        <!-- Run button -->
        <div class="editor-actions">
          <button class="btn-primary" onclick={handlePreview} disabled={running || !sql.trim()}>
            {running ? 'Running...' : '▶  Run Preview (Ctrl+Enter)'}
          </button>
          <span class="row-hint">{preview ? `${preview.row_count} rows returned (showing up to 200)` : ''}</span>
        </div>

        <!-- Save section -->
        <div class="save-section">
          <input
            class="name-input"
            type="text"
            placeholder="Dataset name..."
            bind:value={datasetName}
            disabled={saving}
          />
          <button
            class="btn-success"
            onclick={handleSave}
            disabled={saving || !sql.trim() || !datasetName.trim()}
          >
            {saving ? 'Saving...' : '💾  Save as Dataset'}
          </button>
        </div>

        {#if error}
          <div class="sql-error">{error}</div>
        {/if}
      </div>

      <!-- Right: results -->
      <div class="results-pane">
        {#if running}
          <div class="results-placeholder">Running query...</div>
        {:else if preview}
          <div class="results-header">
            <span>{preview.row_count} row{preview.row_count !== 1 ? 's' : ''}</span>
            <span class="col-count">{preview.columns.length} column{preview.columns.length !== 1 ? 's' : ''}</span>
          </div>
          <div class="results-table-wrap">
            <table class="results-table">
              <thead>
                <tr>
                  {#each preview.columns as col}
                    <th title={col.data_type}>{col.name}<br/><span class="col-type">{col.data_type}</span></th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each preview.rows as row}
                  <tr>
                    {#each row as cell}
                      <td class:null-cell={cell === null}>{cell ?? 'NULL'}</td>
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <div class="results-placeholder">
            <p>Run a query to preview results</p>
            <p class="hint-text">Example:</p>
            <pre class="hint-pre">SELECT name, ST_Area(geometry) AS area
FROM my_dataset
ORDER BY area DESC
LIMIT 10</pre>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .sql-console-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.75);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2000;
  }

  .sql-console {
    background: #16213e;
    border: 1px solid #2a2a4a;
    border-radius: 8px;
    width: 90vw;
    max-width: 1200px;
    height: 85vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .console-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid #2a2a4a;
    flex-shrink: 0;
  }

  .console-header h2 {
    margin: 0;
    font-size: 16px;
  }

  .header-hint {
    color: var(--text-secondary, #888);
    font-size: 12px;
    flex: 1;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-secondary, #888);
    cursor: pointer;
    font-size: 18px;
    padding: 2px 6px;
  }

  .close-btn:hover { color: var(--text, #fff); }

  .console-body {
    display: flex;
    flex: 1;
    overflow: hidden;
    gap: 0;
  }

  /* ── Left: editor ── */
  .editor-pane {
    width: 420px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border-right: 1px solid #2a2a4a;
    overflow-y: auto;
  }

  .section-label {
    font-size: 11px;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .tables-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .table-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .table-chip {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    background: #1a2a4a;
    border: 1px solid #2a3a5a;
    border-radius: 4px;
    padding: 3px 8px;
    cursor: pointer;
    font-size: 11px;
    text-align: left;
  }

  .table-chip:hover { border-color: var(--accent, #4fc3f7); background: #1e3050; }

  .chip-name { color: var(--text, #e0e0e0); font-weight: 500; }
  .chip-table { color: var(--text-secondary, #888); font-family: monospace; font-size: 10px; }

  .no-tables { color: var(--text-secondary, #888); font-size: 12px; }

  .snippets-row {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
  }

  .snippet-btn {
    background: #1e2a3e;
    border: 1px solid #2a3a4a;
    border-radius: 3px;
    padding: 2px 6px;
    font-size: 11px;
    cursor: pointer;
    color: var(--text, #e0e0e0);
  }

  .snippet-btn:hover { border-color: var(--accent, #4fc3f7); }
  .snippet-btn.spatial { border-color: #2a4a3a; color: #7fffb0; }
  .snippet-btn.spatial:hover { border-color: #4fc37f; background: #1e3a2e; }

  .sql-editor {
    flex: 1;
    min-height: 200px;
    background: #0d1626;
    border: 1px solid #2a2a4a;
    border-radius: 4px;
    color: var(--text, #e0e0e0);
    font-family: 'Consolas', 'Monaco', monospace;
    font-size: 13px;
    line-height: 1.5;
    padding: 10px;
    resize: vertical;
  }

  .sql-editor:focus { border-color: var(--accent, #4fc3f7); outline: none; }
  .sql-editor.has-error { border-color: var(--danger, #e74c3c); }

  .editor-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .row-hint { font-size: 12px; color: var(--text-secondary, #888); }

  .save-section {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .name-input {
    flex: 1;
    background: #0d1626;
    border: 1px solid #2a2a4a;
    border-radius: 4px;
    color: var(--text, #e0e0e0);
    font-size: 13px;
    padding: 6px 10px;
  }

  .name-input:focus { border-color: var(--accent, #4fc3f7); outline: none; }

  .btn-success {
    background: #1a4a2e;
    border: 1px solid #2a7a4e;
    border-radius: 4px;
    color: #7fffb0;
    cursor: pointer;
    font-size: 13px;
    padding: 6px 12px;
    white-space: nowrap;
  }

  .btn-success:hover:not(:disabled) { background: #1e5a38; }
  .btn-success:disabled { opacity: 0.5; cursor: not-allowed; }

  .sql-error {
    background: rgba(231, 76, 60, 0.15);
    border: 1px solid var(--danger, #e74c3c);
    border-radius: 4px;
    color: #ff8f8f;
    font-family: monospace;
    font-size: 12px;
    padding: 8px 10px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  /* ── Right: results ── */
  .results-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .results-header {
    display: flex;
    gap: 16px;
    padding: 8px 12px;
    border-bottom: 1px solid #2a2a4a;
    font-size: 12px;
    color: var(--text-secondary, #888);
    flex-shrink: 0;
  }

  .col-count { color: var(--accent, #4fc3f7); }

  .results-table-wrap {
    flex: 1;
    overflow: auto;
  }

  .results-table {
    border-collapse: collapse;
    font-size: 12px;
    font-family: monospace;
    width: max-content;
    min-width: 100%;
  }

  .results-table th {
    background: #0d1626;
    border: 1px solid #2a2a4a;
    padding: 6px 10px;
    text-align: left;
    position: sticky;
    top: 0;
    z-index: 1;
    white-space: nowrap;
    color: var(--text, #e0e0e0);
  }

  .col-type {
    color: var(--text-secondary, #888);
    font-size: 10px;
    font-weight: normal;
  }

  .results-table td {
    border: 1px solid #1e2a3e;
    padding: 4px 10px;
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text, #d0d0d0);
  }

  .results-table tr:hover td { background: #1a2a3e; }

  .null-cell { color: var(--text-secondary, #888); font-style: italic; }

  .results-placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary, #888);
    font-size: 14px;
    gap: 8px;
    padding: 24px;
  }

  .hint-text { font-size: 12px; margin: 0; }

  .hint-pre {
    background: #0d1626;
    border: 1px solid #2a2a4a;
    border-radius: 4px;
    padding: 10px 14px;
    font-size: 12px;
    color: #7fffb0;
    text-align: left;
    margin: 0;
  }
</style>
