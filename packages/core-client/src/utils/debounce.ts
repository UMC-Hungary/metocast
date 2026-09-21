/**
 * Creates a debounced function that delays invoking the provided function
 * until after the specified wait time has elapsed since the last call.
 *
 * @param func - The function to debounce
 * @param wait - The number of milliseconds to delay
 * @returns A debounced version of the function
 */
export function debounce<T extends (...args: never[]) => unknown>(
  func: T,
  wait: number,
): (...args: Parameters<T>) => void {
  let timeout: ReturnType<typeof setTimeout> | null = null;

  return function (this: unknown, ...args: Parameters<T>) {
    if (timeout) {
      clearTimeout(timeout);
    }
    timeout = setTimeout(() => {
      func.apply(this, args);
    }, wait);
  };
}

/**
 * Creates an async debounced function.
 * Cancels any pending invocation when called again within the wait period.
 *
 * @param func - The async function to debounce
 * @param wait - The number of milliseconds to delay
 * @returns A debounced version of the async function
 */
export function debounceAsync<T extends (...args: never[]) => Promise<ReturnType<T>>>(
  func: T,
  wait: number,
): (...args: Parameters<T>) => Promise<ReturnType<T> | undefined> {
  let timeout: ReturnType<typeof setTimeout> | null = null;
  let reject: ((reason?: unknown) => void) | null = null;

  return function (this: unknown, ...args: Parameters<T>): Promise<ReturnType<T> | undefined> {
    // Cancel previous pending call
    if (timeout) {
      clearTimeout(timeout);
    }
    if (reject) {
      reject(new Error('Debounced'));
    }

    return new Promise((resolve, rej) => {
      reject = rej;
      timeout = setTimeout(async () => {
        try {
          const result = await func.apply(this, args);
          resolve(result);
        } catch (error) {
          rej(error);
        } finally {
          reject = null;
        }
      }, wait);
    });
  };
}
