mod service;

use crate::service::{pyn3rd, secrss};

#[tokio::main]
async fn main() {
    // 启动 Web 服务
    let web_app = axum::Router::new()
        .route("/secrss", axum::routing::get(secrss))
        .route("/pyn3rd", axum::routing::get(pyn3rd));
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(web_app.into_make_service())
        .await
        .unwrap();
}
