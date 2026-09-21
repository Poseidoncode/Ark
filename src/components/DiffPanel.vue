<script setup lang="ts">
import { computed } from 'vue';
import { DiffInfo } from "../services/git";

const props = defineProps<{
  diffs: DiffInfo[];
}>();

type LineType = 'added' | 'deleted' | 'hunk' | 'normal';

interface ParsedLine {
  text: string;
  type: LineType;
}

interface ParsedDiff extends DiffInfo {
  lines: { visible: ParsedLine[]; truncated: boolean; count: number };
  ext: string;
  extStyle: { background: string; color: string; border: string };
  changeType: 'added' | 'deleted' | 'modified';
}

const getLineType = (line: string): LineType => {
  if (line.startsWith('+')) return 'added';
  if (line.startsWith('-')) return 'deleted';
  if (line.startsWith('@@')) return 'hunk';
  return 'normal';
};

const parseLine = (line: string) => {
  if (!line) return ' ';
  const firstChar = line.charAt(0);
  if (firstChar === '+' || firstChar === '-' || firstChar === ' ') {
    return line.substring(1) || ' ';
  }
  return line;
};

const MAX_LINES_PER_FILE = 500;

const extColors: Record<string, string> = {
  ts: 'hsl(210, 80%, 55%)', tsx: 'hsl(210, 80%, 55%)',
  js: 'hsl(50, 80%, 45%)', jsx: 'hsl(50, 80%, 45%)',
  vue: 'hsl(140, 70%, 50%)',
  css: 'hsl(270, 60%, 55%)', rs: 'hsl(20, 80%, 55%)',
  json: 'hsl(0, 0%, 50%)', yml: 'hsl(0, 0%, 50%)',
  md: 'hsl(200, 60%, 55%)',
};
const getExtColor = (ext: string) => extColors[ext] || 'hsl(0, 0%, 50%)';

const getExtFileStyle = (path: string) => {
  const parts = path.split('.');
  const ext = parts.length > 1 ? parts[parts.length - 1].toLowerCase() : '';
  const color = getExtColor(ext);
  const bg = color.replace(/(\d+)%/, '15%');
  const border = color.replace(/(\d+)%/, '25%');
  return { ext, style: { background: bg, color, border: `1px solid ${border}` } };
};

// Everything the template needs is computed once per diff list — the old
// template called getLineType() ~6x per line and restyled every file header
// on each render (P3).
const parsedDiffs = computed<ParsedDiff[]>(() => {
  return props.diffs.map(diff => {
    const rawLines = diff.diff_text.split('\n');
    const lineCapped = rawLines.length > MAX_LINES_PER_FILE;
    const visible = (lineCapped ? rawLines.slice(0, MAX_LINES_PER_FILE) : rawLines)
      .map(raw => ({ text: parseLine(raw), type: getLineType(raw) }));
    const { ext, style } = getExtFileStyle(diff.path);
    const changeType = diff.additions > 0 && diff.deletions === 0
      ? 'added'
      : diff.deletions > 0 && diff.additions === 0
        ? 'deleted'
        : 'modified';
    return {
      ...diff,
      lines: { visible, truncated: lineCapped, count: rawLines.length },
      ext,
      extStyle: style,
      changeType,
    };
  });
});
</script>

<template>
  <div v-if="diffs.length === 0"
       class="flex flex-col items-center justify-center p-16 text-muted-foreground gap-3">
    <!-- Empty state icon -->
    <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="opacity-30">
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
      <polyline points="14 2 14 8 20 8"/>
      <line x1="16" y1="13" x2="8" y2="13"/>
      <line x1="16" y1="17" x2="8" y2="17"/>
      <polyline points="10 9 9 9 8 9"/>
    </svg>
    <span class="text-sm">Select a file to view changes</span>
  </div>

  <div v-else class="diff-viewer bg-background select-text">
    <div v-for="diff in parsedDiffs" :key="diff.path" class="diff-file border-b border-border last:border-b-0">

      <!-- File Header -->
      <div class="diff-file-header sticky top-0 z-10 flex items-center justify-between px-4 py-2.5 border-b border-border shadow-sm"
           :style="{ background: 'var(--card)', backdropFilter: 'blur(8px)' }">
        <div class="flex items-center gap-3 min-w-0">
          <!-- File type avatar -->
          <div class="flex-shrink-0 w-6 h-6 rounded flex items-center justify-center text-[9px] font-bold uppercase"
               :style="diff.extStyle">
            {{ diff.ext || 'f' }}
          </div>

          <!-- Change type indicator -->
          <div class="flex-shrink-0">
            <span v-if="diff.changeType === 'added'"
                  class="badge text-[10px] px-1.5 py-0.5 rounded font-bold"
                  style="background: var(--success-bg); color: var(--success)">A</span>
            <span v-else-if="diff.changeType === 'deleted'"
                  class="badge text-[10px] px-1.5 py-0.5 rounded font-bold"
                  style="background: var(--error-bg); color: var(--error)">D</span>
            <span v-else
                  class="badge text-[10px] px-1.5 py-0.5 rounded font-bold"
                  style="background: var(--mark-bg); color: var(--mark)">M</span>
          </div>

          <span class="font-mono text-xs text-foreground font-medium truncate" :title="diff.path">{{ diff.path }}</span>
        </div>

        <div class="flex items-center gap-2 flex-shrink-0 ml-4">
          <span v-if="diff.additions > 0"
                class="font-mono text-[11px] font-bold px-2 py-0.5 rounded-full"
                style="background: var(--success-bg); color: var(--success)">
            +{{ diff.additions }}
          </span>
          <span v-if="diff.deletions > 0"
                class="font-mono text-[11px] font-bold px-2 py-0.5 rounded-full"
                style="background: var(--error-bg); color: var(--error)">
            -{{ diff.deletions }}
          </span>
        </div>
      </div>

      <!-- Diff Lines -->
      <div class="diff-content font-mono text-[12px] leading-5">
        <div v-for="(line, j) in diff.lines.visible" :key="j"
             class="flex group relative"
             :class="{
               'diff-line-added': line.type === 'added',
               'diff-line-deleted': line.type === 'deleted',
               'diff-line-hunk': line.type === 'hunk',
               'diff-line-normal': line.type === 'normal',
             }">
          <!-- Gutter indicator -->
          <span class="diff-gutter w-8 flex-shrink-0 select-none flex items-center justify-center text-[10px] font-bold"
                :class="{
                  'text-success opacity-90': line.type === 'added',
                  'text-error opacity-90': line.type === 'deleted',
                  'opacity-0': line.type === 'normal' || line.type === 'hunk',
                }">
            {{ line.type === 'added' ? '+' : (line.type === 'deleted' ? '−' : '') }}
          </span>

          <!-- Line number -->
          <span class="w-10 text-right pr-3 py-0.5 select-none flex-shrink-0 transition-colors"
                style="color: var(--muted-foreground); opacity: 0.4; font-size: 10px; padding-top: 2px; padding-bottom: 2px;">
            {{ j + 1 }}
          </span>

          <!-- Line content -->
          <span class="flex-1 px-3 py-0.5 whitespace-pre-wrap break-all" style="word-break: break-all;">{{ line.text }}</span>
        </div>

        <!-- Truncation notice -->
        <div v-if="diff.lines.truncated || diff.truncated"
             class="flex items-center justify-center gap-2 py-4 text-[11px] border-t"
             style="color: var(--muted-foreground); border-color: var(--border); background: var(--muted);">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>
          </svg>
          <template v-if="diff.lines.truncated">Showing {{ MAX_LINES_PER_FILE }} of {{ diff.lines.count }} lines — file too large to display fully</template>
          <template v-else>Diff text truncated by size limit — counts reflect the full diff</template>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.diff-line-added {
  background-color: rgba(63, 185, 80, 0.08);
}
.diff-line-added:hover {
  background-color: rgba(63, 185, 80, 0.14);
}
.diff-line-added .diff-gutter {
  border-left: 2px solid var(--success);
}

.diff-line-deleted {
  background-color: rgba(248, 81, 73, 0.08);
}
.diff-line-deleted:hover {
  background-color: rgba(248, 81, 73, 0.14);
}
.diff-line-deleted .diff-gutter {
  border-left: 2px solid var(--error);
}

.diff-line-hunk {
  background-color: rgba(47, 129, 247, 0.06);
  color: var(--muted-foreground);
  font-style: italic;
}
.diff-line-hunk .diff-gutter {
  border-left: 2px solid var(--accent);
}

.diff-line-normal:hover {
  background-color: var(--spotlight);
}
.diff-line-normal .diff-gutter {
  border-left: 2px solid transparent;
}

[data-theme="dark"] .diff-line-added {
  background-color: rgba(63, 185, 80, 0.1);
}
[data-theme="dark"] .diff-line-deleted {
  background-color: rgba(248, 81, 73, 0.1);
}
</style>
