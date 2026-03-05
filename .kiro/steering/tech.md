# Tech Stack

## Frontend

- React 19 with TypeScript 5.8 (strict mode)
- Vite 7 (dev server on port 1420)
- Zustand 5 for state management
- Immer 11 for immutable state updates
- lucide-react for icons
- classnames for conditional CSS classes
- uuid for ID generation

## Backend

- Tauri v2 (Rust)
- tauri-plugin-opener
- serde / serde_json for serialization

## Package Management

- pnpm

## Build & Dev Commands

```bash
# Frontend dev server (used by Tauri automatically)
pnpm dev

# Frontend build (TypeScript check + Vite build)
pnpm build

# Run the full Tauri desktop app in dev mode
pnpm tauri dev

# Build the Tauri app for distribution
pnpm tauri build
```

## TypeScript Config

- Target: ES2020
- Module: ESNext with bundler resolution
- Strict mode enabled (`strict`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`)
- JSX: react-jsx

## Tauri Config

- App window: 800×600, title "tauri-app"
- Frontend dist served from `../dist`
- Dev URL: `http://localhost:1420`
