/**
 * Canvas-based font size calculator for slide text.
 *
 * For each paragraph, text is split on `\n` (matching <a:br> hard breaks from
 * the server). Each resulting line is measured word-accurately with
 * canvas.measureText — no DOM layout, no CSS word-wrap ambiguity.
 *
 * Height is computed mathematically using the same ratios as the CSS:
 *   line-height: 1.2  →  LINE_HEIGHT_RATIO
 *   gap: 0.35em       →  GAP_EM_RATIO
 */

export type FitTextParams = {
  /** Change trigger — Svelte calls update() when this value changes. */
  content?: string;
  /** Pre-computed size: applied directly, skipping calculation. */
  fixedSize?: number | null;
  /** When provided, canvas-based calculation is used instead of DOM heuristics. */
  paragraphs?: { text: string }[];
};

const LINE_HEIGHT_RATIO = 1.2;
const GAP_EM_RATIO = 0.35;
const SLIDE_FONT = '700 {SIZE}px Helvetica, Arial, sans-serif';

/**
 * Find the largest integer font size (px) where every `\n`-separated line of
 * every paragraph fits within `maxW`, and all stacked paragraphs fit `maxH`.
 */
export function calcFontSize(paragraphs: { text: string }[], maxW: number, maxH: number): number {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (!ctx) return 8;

  const nonEmpty = paragraphs.filter((p) => p.text.trim());
  if (nonEmpty.length === 0) return 8;

  // Pre-split lines once outside the binary search loop.
  const allLines: string[][] = nonEmpty.map((p) => p.text.split('\n').filter((l) => l.trim()));

  let lo = 8;
  let hi = Math.max(600, Math.min(Math.floor(maxW), Math.floor(maxH)));

  while (hi - lo > 1) {
    const mid = Math.floor((lo + hi) / 2);
    ctx.font = SLIDE_FONT.replace('{SIZE}', String(mid));

    let fits = true;
    let totalH = 0;

    for (let i = 0; i < allLines.length; i++) {
      const lines = allLines[i] ?? [];
      for (const line of lines) {
        if (ctx.measureText(line).width > maxW) {
          fits = false;
          break;
        }
      }
      if (!fits) break;
      totalH += lines.length * mid * LINE_HEIGHT_RATIO;
      if (i < allLines.length - 1) totalH += mid * GAP_EM_RATIO;
    }

    if (fits && totalH <= maxH) {
      lo = mid;
    } else {
      hi = mid;
    }
  }

  return lo;
}

export function fitText(
  node: HTMLElement,
  params?: FitTextParams,
): { update: (p?: FitTextParams) => void; destroy: () => void } {
  let currentFixedSize: number | null | undefined = params?.fixedSize;
  let currentParagraphs: { text: string }[] | undefined = params?.paragraphs;

  function getContainerDims(): { maxW: number; maxH: number } | null {
    const parent = node.parentElement;
    if (!parent) return null;
    const style = getComputedStyle(parent);
    const paddingH = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    const paddingW = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
    const parentH = parent.clientHeight - paddingH;
    const parentW = parent.clientWidth - paddingW;
    if (parentH <= 0 || parentW <= 0) return null;
    return { maxW: Math.ceil(parentW), maxH: Math.ceil(parentH) };
  }

  function fit() {
    if (currentFixedSize != null) {
      node.style.fontSize = `${currentFixedSize}px`;
      return;
    }

    const dims = getContainerDims();
    if (!dims) return;
    const { maxW, maxH } = dims;

    if (currentParagraphs && currentParagraphs.length > 0) {
      node.style.fontSize = `${calcFontSize(currentParagraphs, maxW, maxH)}px`;
      return;
    }

    // DOM fallback for non-paragraph use-cases.
    let lo = 8;
    let hi = Math.max(600, Math.min(maxW, maxH));
    node.style.fontSize = `${hi}px`;
    while (hi - lo > 1) {
      const mid = Math.floor((lo + hi) / 2);
      node.style.fontSize = `${mid}px`;
      if (node.scrollHeight > maxH || node.scrollWidth > maxW) {
        hi = mid;
      } else {
        lo = mid;
      }
    }
    node.style.fontSize = `${lo}px`;
  }

  const observer = new ResizeObserver(fit);
  observer.observe(node);
  if (node.parentElement) observer.observe(node.parentElement);
  fit();

  return {
    update(newParams?: FitTextParams) {
      currentFixedSize = newParams?.fixedSize;
      currentParagraphs = newParams?.paragraphs;
      fit();
    },
    destroy() {
      observer.disconnect();
    },
  };
}
