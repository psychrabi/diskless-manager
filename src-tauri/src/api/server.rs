use std::net::SocketAddr;
use tokio::sync::oneshot;
use tracing::info;

use crate::state::AppState;

pub struct ApiServer {
    addr: SocketAddr,
    app: axum::Router,
}

impl ApiServer {
    pub fn new(state: AppState, addr: SocketAddr) -> Self {
        let app = crate::api::routes::create_app(state);
        Self { addr, app }
    }

    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting Axum API server on {}", self.addr);

        let listener = tokio::net::TcpListener::bind(self.addr).await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        axum::serve(listener, self.app)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    pub async fn start_with_shutdown(self, shutdown_rx: oneshot::Receiver<()>) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting Axum API server on {} with shutdown signal", self.addr);

        let listener = tokio::net::TcpListener::bind(self.addr).await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        let server = axum::serve(listener, self.app);

        let graceful = server.with_graceful_shutdown(async {
            shutdown_rx.await.ok();
        });

        graceful.await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}