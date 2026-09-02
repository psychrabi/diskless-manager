use log::info;
use std::net::SocketAddr;
use tokio::sync::oneshot;

use crate::state::AppState;

pub fn api_address(configured: Option<&str>) -> anyhow::Result<SocketAddr> {
    let value = configured.unwrap_or("127.0.0.1:8080");
    value
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid API bind address '{value}': {error}"))
}

async fn bind_listener(addr: SocketAddr) -> anyhow::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| anyhow::anyhow!("failed to bind API server to {addr}: {error}"))
}

pub struct ApiServer {
    addr: SocketAddr,
    app: axum::Router,
}

pub struct BoundApiServer {
    listener: tokio::net::TcpListener,
    app: axum::Router,
}

impl ApiServer {
    pub fn new(state: AppState, addr: SocketAddr) -> Self {
        let app = crate::api::routes::create_app(state);
        Self { addr, app }
    }

    pub async fn bind(self) -> anyhow::Result<BoundApiServer> {
        let listener = bind_listener(self.addr).await?;
        info!("API server bound to {}", listener.local_addr()?);
        Ok(BoundApiServer {
            listener,
            app: self.app,
        })
    }

    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting Axum API server on {}", self.addr);

        let bound = self.bind().await?;
        axum::serve(bound.listener, bound.app)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    pub async fn start_with_shutdown(
        self,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Starting Axum API server on {} with shutdown signal",
            self.addr
        );

        let bound = self.bind().await?;
        let server = axum::serve(bound.listener, bound.app);

        let graceful = server.with_graceful_shutdown(async {
            shutdown_rx.await.ok();
        });

        graceful
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl BoundApiServer {
    pub async fn serve_with_shutdown(
        self,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> anyhow::Result<()> {
        axum::serve(self.listener, self.app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{api_address, bind_listener};

    #[test]
    fn api_address_defaults_to_loopback() {
        assert_eq!(
            api_address(None).expect("default address should be valid"),
            "127.0.0.1:8080".parse().unwrap()
        );
    }

    #[test]
    fn api_address_accepts_an_explicit_bind_address() {
        assert_eq!(
            api_address(Some("0.0.0.0:9090")).expect("configured address should be valid"),
            "0.0.0.0:9090".parse().unwrap()
        );
    }

    #[test]
    fn api_address_rejects_invalid_configuration() {
        let error = api_address(Some("not-an-address")).expect_err("invalid address must fail");
        assert!(error.to_string().contains("not-an-address"));
    }

    #[tokio::test]
    async fn binding_reports_an_occupied_address() {
        let occupied = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("test port should bind: {error}"),
        };
        let address = occupied.local_addr().expect("test address should exist");

        let error = bind_listener(address)
            .await
            .expect_err("second listener must be rejected");

        assert!(error.to_string().contains(&address.to_string()));
    }
}
