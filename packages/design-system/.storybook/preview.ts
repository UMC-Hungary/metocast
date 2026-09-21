import type { Preview } from '@storybook/svelte-vite';
import '../static/fonts/fonts.css';
import '../tokens/generated/tokens.css';
import './catalog.css';

/** The reference's own reflow thresholds, so a story is never viewed at a width the design never has. */
const viewports = {
  narrow: { name: 'Narrow phone (360px)', styles: { width: '360px', height: '740px' } },
  phone: { name: 'Phone (402px)', styles: { width: '402px', height: '874px' } },
  tablet: { name: 'Tablet (760px)', styles: { width: '760px', height: '1024px' } },
  rail: { name: 'Rail (980px)', styles: { width: '980px', height: '900px' } },
  wide: { name: 'Wide (1360px)', styles: { width: '1360px', height: '900px' } },
};

const preview: Preview = {
  parameters: {
    controls: { matchers: { color: /(background|colou?r)$/i, date: /Date$/i } },
    viewport: { options: viewports },
    // Every story is checked; a violation fails the run rather than being reported and ignored.
    a11y: {
      test: 'error',
      options: {
        rules: {
          // Off by default in axe's WCAG-only set, and it is exactly the rule that catches the
          // 32px targets the reference is full of.
          'target-size': { enabled: true },
        },
      },
    },
    backgrounds: { disabled: true },
  },
  initialGlobals: {
    viewport: { value: 'phone' },
  },
  globalTypes: {
    scheme: {
      description: 'Colour scheme',
      toolbar: {
        title: 'Scheme',
        icon: 'circlehollow',
        items: [
          { value: 'light', title: 'Light' },
          { value: 'dark', title: 'Dark' },
        ],
        dynamicTitle: true,
      },
    },
    surface: {
      description: 'Surface the component sits on',
      toolbar: {
        title: 'Surface',
        icon: 'box',
        items: [
          { value: 'base', title: 'Base' },
          { value: 'raised', title: 'Raised' },
          { value: 'sunken', title: 'Sunken' },
        ],
        dynamicTitle: true,
      },
    },
  },
  decorators: [
    (story, context) => {
      // The attribute is what the token stylesheet keys off, so switching it here exercises
      // exactly the mechanism a consuming application uses — no reload, no remount.
      document.documentElement.setAttribute('data-scheme', context.globals.scheme ?? 'light');
      document.body.dataset.surface = context.globals.surface ?? 'base';
      return story();
    },
  ],
};

export default preview;
