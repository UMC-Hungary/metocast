// Hand-written: FormField hands its control the ids and invalid flag through a snippet parameter,
// which a plain Storybook arg cannot express — and showing it without a control proves nothing.
import type { Meta, StoryObj } from '@storybook/svelte-vite';
import FormField from '../src/primitives/FormField.svelte';
import Anatomy from './support/Anatomy.svelte';
import FormFieldPreview from './support/FormFieldPreview.svelte';

const meta = {
  title: 'Primitives/FormField',
  component: FormField,
  tags: ['autodocs'],
  parameters: {
    docs: { description: { component: 'Specification: docs/FormField.md' } },
  },
} satisfies Meta<FormField>;

export default meta;
type Story = StoryObj<typeof meta>;

const preview = (props: Record<string, unknown>) => () => ({
  Component: FormFieldPreview,
  props,
});

export const Default: Story = {
  render: preview({ label: 'Kiszolgáló címe', value: '192.168.1.10' }),
};

export const WithHint: Story = {
  render: preview({
    label: 'Kiszolgáló címe',
    hint: 'Például http://192.168.1.10:1421',
    value: '192.168.1.10',
  }),
};

export const Required: Story = {
  render: preview({ label: 'Esemény neve', required: true, value: '' }),
};

export const Invalid: Story = {
  render: preview({ label: 'Kiszolgáló címe', error: 'Nem érvényes cím', value: '192.168' }),
};

/** The same wrapper around a text area and a select, to show the wiring is control-agnostic. */
export const AroundATextArea: Story = {
  render: preview({ label: 'Megjegyzés', control: 'textarea', value: 'A tolmács a karzaton ül.' }),
};

export const AroundASelect: Story = {
  render: preview({
    label: 'Jelenet',
    control: 'select',
    hint: 'Az adás indulásakor ez lesz aktív',
  }),
};

export const Anatomy_: Story = {
  name: 'Anatomy & reference',
  render: () => ({ Component: Anatomy, props: { component: 'FormField' } }),
};
