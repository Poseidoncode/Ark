import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DiffPanel from '../DiffPanel.vue';
import type { DiffInfo } from '../../services/git';

const makeDiff = (overrides: Partial<DiffInfo> = {}): DiffInfo => ({
  path: 'src/a.ts',
  additions: 1,
  deletions: 1,
  diff_text: '+added\n-removed\n context',
  truncated: false,
  ...overrides,
});

describe('DiffPanel', () => {
  it('classifies each line once and renders display text without prefixes', () => {
    const wrapper = mount(DiffPanel, { props: { diffs: [makeDiff()] } });

    expect(wrapper.findAll('.diff-line-added').length).toBe(1);
    expect(wrapper.findAll('.diff-line-deleted').length).toBe(1);
    expect(wrapper.findAll('.diff-line-normal').length).toBe(1);
    expect(wrapper.text()).toContain('added');
    expect(wrapper.text()).not.toContain('+added');
  });

  it('shows a size-limit notice when the backend truncated the text', () => {
    const wrapper = mount(DiffPanel, { props: { diffs: [makeDiff({ truncated: true })] } });

    expect(wrapper.text()).toContain('truncated by size limit');
  });

  it('caps rendered lines per file with a count notice', () => {
    const big = Array.from({ length: 600 }, (_, i) => ` line${i}`).join('\n');
    const wrapper = mount(DiffPanel, {
      props: { diffs: [makeDiff({ diff_text: big, additions: 0, deletions: 0 })] },
    });

    expect(wrapper.findAll('.diff-line-normal').length).toBe(500);
    expect(wrapper.text()).toContain('Showing 500 of 600 lines');
  });

  it('renders an empty state when there are no diffs', () => {
    const wrapper = mount(DiffPanel, { props: { diffs: [] } });

    expect(wrapper.text()).toContain('Select a file to view changes');
  });
});
