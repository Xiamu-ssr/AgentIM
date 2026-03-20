//! 嵌入前端静态资源并通过 Axum 提供服务
//!
//! 使用 rust-embed 将 `frontend/out/` 目录嵌入到二进制文件中。
//! 如果构建时目录不存在，则嵌入空资源（开发模式）。

use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../frontend/out/"]
struct FrontendAssets;

/// Axum handler: serve embedded frontend static files.
/// Falls back to `index.html` for SPA client-side routing.
pub async fn static_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try exact path first.
    if let Some(content) = FrontendAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            content.data.to_vec(),
        )
            .into_response();
    }

    // Try path with index.html (for directory routes like /dashboard/).
    let index_path = if path.is_empty() {
        "index.html".to_string()
    } else if path.ends_with('/') {
        format!("{}index.html", path)
    } else {
        format!("{}/index.html", path)
    };

    if let Some(content) = FrontendAssets::get(&index_path) {
        let mime = mime_guess::from_path(&index_path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            content.data.to_vec(),
        )
            .into_response();
    }

    // SPA fallback: serve root index.html for client-side routing.
    if let Some(content) = FrontendAssets::get("index.html") {
        return Html(String::from_utf8_lossy(&content.data).to_string()).into_response();
    }

    // No frontend assets embedded (dev mode without frontend build).
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}
