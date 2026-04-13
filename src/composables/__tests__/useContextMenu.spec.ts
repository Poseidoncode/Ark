import { describe, it, expect, vi } from 'vitest';
import { useContextMenu } from '../useContextMenu';

describe('useContextMenu', () => {
  it('應該初始化為隱藏狀態', () => {
    const { isVisible, menuItems, position } = useContextMenu();
    
    expect(isVisible.value).toBe(false);
    expect(menuItems.value).toEqual([]);
    expect(position.x).toBe(0);
    expect(position.y).toBe(0);
  });

  it('應該顯示 context menu', () => {
    const { isVisible, position, menuItems, showContextMenu } = useContextMenu();
    
    const mockEvent = new MouseEvent('contextmenu', {
      clientX: 100,
      clientY: 200,
    });
    
    const items = [
      { label: 'Copy', action: () => {} },
      { label: 'Paste', action: () => {} },
    ];
    
    showContextMenu(mockEvent, items);
    
    expect(isVisible.value).toBe(true);
    expect(position.x).toBe(100);
    expect(position.y).toBe(200);
    expect(menuItems.value).toHaveLength(2);
  });

  it('應該隱藏 context menu', () => {
    const { isVisible, menuItems, showContextMenu, hideContextMenu } = useContextMenu();
    
    const mockEvent = new MouseEvent('contextmenu', {
      clientX: 100,
      clientY: 200,
    });
    
    showContextMenu(mockEvent, [{ label: 'Test' }]);
    expect(isVisible.value).toBe(true);
    
    hideContextMenu();
    
    expect(isVisible.value).toBe(false);
    expect(menuItems.value).toEqual([]);
  });

  it('應該支持 divider 菜單項', () => {
    const { menuItems, showContextMenu } = useContextMenu();
    
    const mockEvent = new MouseEvent('contextmenu');
    const items = [
      { label: 'Copy', action: () => {} },
      { divider: true },
      { label: 'Delete', action: () => {}, danger: true },
    ];
    
    showContextMenu(mockEvent, items);
    
    expect(menuItems.value[1].divider).toBe(true);
    expect(menuItems.value[2].danger).toBe(true);
  });

  it('應該阻止默認的右鍵菜單', () => {
    const { showContextMenu } = useContextMenu();
    
    const mockEvent = new MouseEvent('contextmenu');
    const preventDefaultSpy = vi.spyOn(mockEvent, 'preventDefault');
    
    showContextMenu(mockEvent, [{ label: 'Test' }]);
    
    expect(preventDefaultSpy).toHaveBeenCalled();
  });
});
