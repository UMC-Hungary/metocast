// Hand-written: Tabs renders its panel through a snippet that receives the selected value, which
// a plain Storybook arg cannot express.
import type { Meta, StoryObj } from '@storybook/svelte-vite';
import Tabs from '../src/primitives/Tabs.svelte';
import Anatomy from './support/Anatomy.svelte';
import TabsPreview from './support/TabsPreview.svelte';

const meta = {
  title: 'Primitives/Tabs',
  component: Tabs,
  tags: ['autodocs'],
  parameters: {
    docs: { description: { component: 'Specification: docs/Tabs.md' } },
  },
} satisfies Meta<Tabs>;

export default meta;
type Story = StoryObj<typeof meta>;

const tabs = [
  { value: 'details', label: 'Részletek' },
  { value: 'readings', label: 'Igehelyek' },
  { value: 'stream', label: 'Közvetítés' },
];

export const Default: Story = {
  render: () => ({ Component: TabsPreview, props: { tabs, value: 'details' } }),
};

export const SecondSelected: Story = {
  render: () => ({ Component: TabsPreview, props: { tabs, value: 'readings' } }),
};

export const WithDisabled: Story = {
  render: () => ({
    Component: TabsPreview,
    props: {
      value: 'details',
      tabs: [...tabs.slice(0, 2), { value: 'stream', label: 'Közvetítés', disabled: true }],
    },
  }),
};

/** Many tabs, so the strip scrolls rather than wrapping or shrinking its targets. */
export const Overflowing: Story = {
  render: () => ({
    Component: TabsPreview,
    props: {
      value: 'details',
      tabs: [
        ...tabs,
        { value: 'uploads', label: 'Feltöltések' },
        { value: 'connectors', label: 'Csatlakozók' },
        { value: 'log', label: 'Napló' },
      ],
    },
  }),
};

export const Anatomy_: Story = {
  name: 'Anatomy & reference',
  render: () => ({ Component: Anatomy, props: { component: 'Tabs' } }),
};
