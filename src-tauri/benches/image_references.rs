//! Fresh-database samples for client dependency checks, including matching and missing references.
use app_lib::{core::image::Image, persistence::repositories::image::ImageRepository};
use std::{hint::black_box, time::Instant};

fn image() -> anyhow::Result<Image> {
    Ok(serde_json::from_value(serde_json::json!({
        "id":"master", "name":"diskless/image", "kind":"master", "os_type":"windows",
        "size_gb":20, "path":"/dev/zvol/diskless/image", "format":"raw", "status":"ready",
        "description":null, "parent_id":null, "source_snapshot":null, "checksum":null,
        "is_default":false, "created_at":"2026-01-01T00:00:00Z", "updated_at":"2026-01-01T00:00:00Z"
    }))?)
}

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        println!("| Clients | Reference | Median ms | Min ms | Max ms | Samples |");
        println!("|---:|:---|---:|---:|---:|---:|");
        for clients in [100, 10_000, 100_000] {
            for present in [true, false] {
                let mut samples = Vec::with_capacity(11);
                for _ in 0..11 {
                    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
                        .connect("sqlite::memory:").await?;
                    sqlx::migrate!("./migrations").run(&pool).await?;
                    sqlx::query("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i+1 < ?) INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at) SELECT 'pc-'||i,'PC'||i,'mac-'||i,'ip-'||i,'diskless/image',1,'now','now' FROM n")
                        .bind(clients).execute(&pool).await?;
                    let repository = ImageRepository::new(pool.clone());
                    let mut image = image()?;
                    if !present { image.name = "diskless/missing".into(); }
                    let start = Instant::now();
                    let found = repository.has_client_references(&image).await?;
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    assert_eq!(found, present);
                    black_box(found);
                    samples.push(elapsed);
                    pool.close().await;
                }
                samples.sort_by(f64::total_cmp);
                println!("| {clients} | {} | {:.3} | {:.3} | {:.3} | 11 |",
                    if present { "present" } else { "absent" }, samples[5],samples[0],samples[10]);
            }
        }
        Ok(())
    })
}
