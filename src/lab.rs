use axum::{
    http::{HeaderValue, StatusCode, header::CONTENT_TYPE},
    response::{Html, IntoResponse, Response},
};

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../static/lab/index.html"))
}

pub async fn app_js() -> Response {
    asset(
        "application/javascript",
        include_str!("../static/lab/app.js"),
    )
}

pub async fn styles_css() -> Response {
    asset("text/css", include_str!("../static/lab/styles.css"))
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}
