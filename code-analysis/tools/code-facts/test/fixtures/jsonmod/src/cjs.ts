import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

// Only `require`d, so TypeScript never puts it in the program.
export const settings = (): unknown => require('./settings.json');
