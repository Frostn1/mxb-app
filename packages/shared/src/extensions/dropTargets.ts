/** Returns whether a Tauri physical pointer position is over a named HTML drop target. */
export function isOverDropTarget(position: { x: number; y: number }, name: string): boolean {
  const scale = window.devicePixelRatio || 1;
  const x = position.x / scale;
  const y = position.y / scale;

  return Array.from(document.querySelectorAll<HTMLElement>("[data-drop-target]"))
    .filter((element) => element.dataset.dropTarget === name)
    .some((element) => {
      const bounds = element.getBoundingClientRect();
      return x >= bounds.left && x <= bounds.right && y >= bounds.top && y <= bounds.bottom;
    });
}
