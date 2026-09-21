/** Overlay stack, so Escape and an outside click only ever reach the topmost layer. */
const layers: HTMLElement[] = [];

export interface DismissableOptions {
  ondismiss: () => void;
  /** Set false for overlays that must be dismissed explicitly, such as a destructive confirm. */
  closeOnOutsideClick?: boolean;
  closeOnEscape?: boolean;
}

export function dismissable(node: HTMLElement, options: DismissableOptions) {
  let opts = { closeOnOutsideClick: true, closeOnEscape: true, ...options };
  layers.push(node);

  const isTopmost = () => layers[layers.length - 1] === node;

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape' || !opts.closeOnEscape || !isTopmost()) return;
    event.stopPropagation();
    opts.ondismiss();
  }

  function onPointerdown(event: PointerEvent) {
    if (!opts.closeOnOutsideClick || !isTopmost()) return;
    if (!node.contains(event.target as Node)) {
      opts.ondismiss();
      document.addEventListener(
        'click',
        (e) => {
          e.stopPropagation();
          e.preventDefault();
        },
        { capture: true, once: true },
      );
    }
  }

  document.addEventListener('keydown', onKeydown, true);
  // Pointerdown rather than click, so a press that starts outside cannot be swallowed.
  document.addEventListener('pointerdown', onPointerdown, true);

  return {
    update(next: DismissableOptions) {
      opts = { ...opts, ...next };
    },
    destroy() {
      document.removeEventListener('keydown', onKeydown, true);
      document.removeEventListener('pointerdown', onPointerdown, true);
      const index = layers.indexOf(node);
      if (index !== -1) layers.splice(index, 1);
    },
  };
}
