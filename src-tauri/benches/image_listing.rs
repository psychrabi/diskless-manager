//! Independent end-to-end image-listing samples: fresh SQLite database per sample.
use app_lib::{
    api::handlers::images::list_masters, application::ApplicationServices, core::config::Settings,
    metrics::MetricsCollector, ssh_executor::SshExecutor, state::AppState,
};
use axum::extract::State;
use std::{hint::black_box, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::{Mutex, RwLock};

async fn fixture(masters: usize) -> anyhow::Result<AppState> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let mut transaction = pool.begin().await?;
    for master in 0..masters {
        for child in 0..5 {
            let id = format!("{master:06}-{child}");
            let parent = (child != 0).then(|| format!("{master:06}-0"));
            sqlx::query("INSERT INTO images (id,name,kind,os_type,size_gb,path,format,status,parent_id,is_default,created_at,updated_at) VALUES (?,?,?,'windows',20,?,'raw','ready',?,0,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')")
                .bind(&id).bind(&id).bind(if child == 0 { "master" } else { "snapshot" })
                .bind(format!("/dev/zvol/diskless/image-{master:06}"))
                .bind(parent).execute(&mut *transaction).await?;
        }
    }
    transaction.commit().await?;
    Ok(AppState {
        client_mutations: Arc::new(Mutex::new(())),
        settings: Arc::new(RwLock::new(Settings::default())),
        db_pool: pool.clone(),
        config_path: PathBuf::new(),
        client_ips: Arc::new(RwLock::new(Vec::new())),
        metrics_collector: Arc::new(MetricsCollector::default()),
        ssh_executor: Arc::new(SshExecutor::new()),
        application: Arc::new(ApplicationServices::new(pool)),
    })
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        println!("| Masters | Total records | Median ms | Min ms | Max ms | Samples |");
        println!("|---:|---:|---:|---:|---:|---:|");
        for masters in [50, 500, 5_000] {
            let mut samples = Vec::with_capacity(11);
            for _ in 0..11 {
                let state = fixture(masters).await?;
                let pool = state.db_pool.clone();
                let start = Instant::now();
                let response = list_masters(State(state))
                    .await
                    .map_err(|status| anyhow::anyhow!("handler failed: {status}"))?;
                let bytes = serde_json::to_vec(&response.0)?;
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(response.0.len(), masters);
                assert!(response.0.iter().all(|master| master.snapshots.len() == 4));
                black_box(bytes);
                samples.push(elapsed);
                pool.close().await;
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "| {masters} | {} | {:.3} | {:.3} | {:.3} | 11 |",
                masters * 5,
                samples[5],
                samples[0],
                samples[10]
            );
        }
        Ok(())
    })
}
