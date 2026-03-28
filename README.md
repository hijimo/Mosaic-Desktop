# Mosaic Desktop

## 开发规范

- 当前 UI 主样式系统是 `MUI + theme + sx`。业务组件继续使用 MUI，不把 Tailwind 当作通用样式方案。
- Tailwind CSS 在本项目中的用途被明确限定为：支持 `streamdown` / `@streamdown/code` 的官方默认样式生成。
- 除非有单独的架构决策，不要在业务组件中新增 Tailwind utility class，不要把页面样式迁移到 Tailwind。
- `src/styles/global.css` 中的 `@source "../../node_modules/streamdown/dist/*.js"` 和 `@source "../../node_modules/@streamdown/code/dist/*.js"` 是 `streamdown` 官方默认样式所需配置，修改前需要确认不会影响 Markdown 渲染。
- `src/styles/global.css` 中的 shadcn-compatible CSS variables 是为了让 `streamdown` 默认代码块/卡片样式正常工作，不是项目级主题迁移的开始。

## 运行

- `pnpm dev`
- `pnpm build`
- `pnpm test`

## 分享功能环境变量

- 分享页上传只允许 Rust/Tauri 侧读取 OSS 凭证，前端不要读取或透传 AK/SK。
- 推荐使用 `MOSAIC_OSS_ACCESSKEY_ID`、`MOSAIC_OSS_ACCESSKEY_SECRET`、`MOSAIC_OSS_BUCKET`、`MOSAIC_OSS_REGION`、`MOSAIC_OSS_DIST`、`MOSAIC_OSS_HOST`。
- 为了兼容现有本地配置，Rust 侧会回退读取 `VITE_OSS_ACCESSKEY_ID`、`VITE_OSS_ACCESSKEY_SECRET`、`VITE_OSS_BUCKET`、`VITE_OSS_REGION`、`VITE_OSS_DIST`、`VITE_OSS_HOST`。
- `MOSAIC_OSS_DIST` / `VITE_OSS_DIST` 用于分享文件前缀目录，例如 `ai-share/`。
