declare module 'vue-virtual-scroller' {
  import type { Plugin } from 'vue';
  import type { DefineComponent } from 'vue';

  const plugin: Plugin;
  export default plugin;

  // RecycleScroller component props
  interface RecycleScrollerProps {
    items: unknown[];
    itemSize?: number;
    keyField?: string;
    direction?: 'vertical' | 'horizontal';
    listTag?: string;
    itemTag?: string;
    buffer?: number;
    [key: string]: unknown;
  }

  export const RecycleScroller: DefineComponent<RecycleScrollerProps>;
}
