import { defineConfig } from 'vitest/config';
import { config } from 'dotenv';

// Load .env.test so TAURI_TEST_TOKEN is available without manual export.
// Using test.env (not process.env assignment) ensures the value is injected
// into every vitest worker process, not just the main config process.
// The same token value is used by globalSetup when spawning the server,
// so the server and test clients always agree on the auth token.
const { parsed: envFile = {} } = config({ path: '.env.test' });

export default defineConfig({
  test: {
    include: ['e2e/**/*.test.ts'],
    // Every file drives the same server and the same database, so tests that
    // touch a singleton row (connector config, app settings) race each other
    // when files run in parallel. Within a file, tests are already sequential.
    fileParallelism: false,
    testTimeout: 15000,
    globals: true,
    env: envFile,
    globalSetup: ['./e2e/global-setup.ts'],
  },
});
