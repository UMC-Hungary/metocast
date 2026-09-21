export interface RovingOptions {
  /** Matches the items that take part in the roving sequence. */
  selector?: string;
  orientation?: 'horizontal' | 'vertical' | 'both';
  wrap?: boolean;
  /** Called with the index the arrow keys landed on. */
  onmove?: (index: number) => void;
}

/**
 * One tab stop for a group of controls; arrows move within it, Home and End jump to the ends.
 * The APG roving tabindex pattern, shared by Tabs, Segmented, RadioGroup and the tab bar.
 */
export function rovingTabindex(node: HTMLElement, options: RovingOptions = {}) {
  let opts = { selector: '[data-roving]', orientation: 'horizontal', wrap: true, ...options };

  const items = () => [...node.querySelectorAll<HTMLElement>(opts.selector)];

  function focusAt(index: number) {
    const all = items();
    if (all.length === 0) return;
    const next = opts.wrap
      ? (index + all.length) % all.length
      : Math.min(Math.max(index, 0), all.length - 1);
    all[next]?.focus();
    opts.onmove?.(next);
  }

  function onKeydown(event: KeyboardEvent) {
    const all = items();
    const current = all.indexOf(document.activeElement as HTMLElement);
    if (current === -1) return;
    const horizontal = opts.orientation !== 'vertical';
    const vertical = opts.orientation !== 'horizontal';
    const step = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: 1, ArrowUp: -1 }[event.key];
    const axisAllowed =
      ((event.key === 'ArrowRight' || event.key === 'ArrowLeft') && horizontal) ||
      ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && vertical);

    if (step !== undefined && axisAllowed) {
      event.preventDefault();
      focusAt(current + step);
    } else if (event.key === 'Home') {
      event.preventDefault();
      focusAt(0);
    } else if (event.key === 'End') {
      event.preventDefault();
      focusAt(all.length - 1);
    }
  }

  node.addEventListener('keydown', onKeydown);
  return {
    update(next: RovingOptions) {
      opts = { ...opts, ...next };
    },
    destroy() {
      node.removeEventListener('keydown', onKeydown);
    },
  };
}
