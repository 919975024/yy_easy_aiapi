/// 加载并返回嵌入的 OpenAPI 3.0 规范 JSON
pub fn openapi_json() -> serde_json::Value {
    serde_json::from_str(include_str!("openapi.json"))
        .expect("OpenAPI JSON 文件格式错误")
}

/// GET /api-doc/openapi.json — 返回 OpenAPI 规范 JSON
pub async fn openapi_spec() -> axum::Json<serde_json::Value> {
    axum::Json(openapi_json())
}

/// GET /swagger — Swagger UI 页面
pub async fn swagger_ui() -> axum::response::Html<&'static str> {
    axum::response::Html(SWAGGER_HTML)
}

const SWAGGER_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>yy_easy_aiapi — API 文档</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css">
  <style>
    html { box-sizing: border-box; overflow-y: scroll; }
    *, *:before, *:after { box-sizing: inherit; }
    body { margin: 0; background: #fafafa; }
    .topbar { display: none; }
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({
      url: '/yy_easy_aiapi/api-doc/openapi.json',
      dom_id: '#swagger-ui',
      deepLinking: true,
      docExpansion: 'list',
      defaultModelsExpandDepth: -1,
      filter: true,
      showExtensions: true,
      showCommonExtensions: true,
      tryItOutEnabled: false,
      syntaxHighlight: { activate: true, theme: 'monokai' },
    });
  </script>
</body>
</html>"#;