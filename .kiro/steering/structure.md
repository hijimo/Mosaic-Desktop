# Project Structure

```
├── src/                    # React frontend (TypeScript)
│   ├── main.tsx            # App entry point, renders root
│   ├── App.tsx             # Main application component
│   ├── App.css             # Application styles
│   ├── assets/             # Static assets (SVGs, images)
│   └── vite-env.d.ts       # Vite type declarations
│
├── src-tauri/              # Tauri backend (Rust)
│   ├── src/
│   │   ├── main.rs         # Desktop entry point
│   │   └── lib.rs          # Tauri commands and plugin setup
│   ├── capabilities/       # Tauri permission capabilities
│   ├── icons/              # App icons for all platforms
│   ├── Cargo.toml          # Rust dependencies
│   └── tauri.conf.json     # Tauri app configuration
│
├── public/                 # Static files served as-is
├── docs/                   # Reference documentation (Codex CLI study notes, not this app)
│
├── index.html              # HTML entry point
├── package.json            # Node dependencies and scripts
├── vite.config.ts          # Vite configuration
├── tsconfig.json           # TypeScript config (frontend)
└── tsconfig.node.json      # TypeScript config (Node/Vite)
```

## Conventions

- Frontend code lives in `src/`, backend Rust code in `src-tauri/src/`
- Tauri commands are defined in `src-tauri/src/lib.rs` and invoked from React via `@tauri-apps/api/core`
- The `docs/` folder is reference material only — not documentation for this project
