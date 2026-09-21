// Hand-written: Tooltip hands its trigger the `describedby` it must apply, through a snippet
// parameter — and a tooltip with no trigger cannot be shown at all.
import type { Meta, StoryObj } from '@storybook/svelte-vite';
import Tooltip from '../src/primitives/Tooltip.svelte';
import Anatomy from './support/Anatomy.svelte';
import TooltipPreview from './support/TooltipPreview.svelte';

const meta = {
  title: 'Primitives/Tooltip',
  component: Tooltip,
  tags: ['autodocs'],
  parameters: {
    docs: { description: { component: 'Specification: docs/Tooltip.md' } },
  },
} satisfies Meta<Tooltip>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Top: Story = {
  render: () => ({
    Component: TooltipPreview,
    props: { text: 'A kódoló újraindítása megszakítja az adást.', placement: 'top' },
  }),
};

export const Bottom: Story = {
  render: () => ({
    Component: TooltipPreview,
    props: { text: 'A kódoló újraindítása megszakítja az adást.', placement: 'bottom' },
  }),
};

/** Longer text, to show the bubble wraps rather than running off the viewport. */
export const LongText: Story = {
  render: () => ({
    Component: TooltipPreview,
    props: {
      text: 'Az újraindítás körülbelül nyolc másodpercre megszakítja az adást, és a nézők ez alatt szünetet látnak.',
      placement: 'bottom',
    },
  }),
};

export const Anatomy_: Story = {
  name: 'Anatomy & reference',
  render: () => ({ Component: Anatomy, props: { component: 'Tooltip' } }),
};
