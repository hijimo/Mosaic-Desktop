import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

// Smoke test: only runs when explicitly triggered via `pnpm test:smoke`
if (import.meta.env.VITE_RUN_SMOKE === 'true' && (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__) {
  import('./utils/smoke').then(({ runSmoke }) => {
    setTimeout(async () => {
      const result = await runSmoke();
      const el = document.createElement('div');
      el.id = 'smoke-result';
      el.setAttribute('data-success', String(result.success));
      el.setAttribute('data-events', JSON.stringify(result.events));
      el.setAttribute('data-error', result.error ?? '');
      el.style.display = 'none';
      document.body.appendChild(el);
    }, 2000);
  });
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
