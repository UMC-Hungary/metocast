export async function isGlassSupported(): Promise<boolean> {
  return false;
}

export async function setLiquidGlassEffect(_opts: {
  enabled?: boolean;
  cornerRadius?: number;
  tintColor?: string;
  variant?: number;
}): Promise<void> {}
