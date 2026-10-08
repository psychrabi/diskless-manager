//! Client inventory queries with a fresh database for every independent sample.
use app_lib::persistence::repositories::client::ClientRepository;
use std::{hint::black_box, time::Instant};

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            println!("| Clients | Median ms | Min ms | Max ms | Samples |");
            println!("|---:|---:|---:|---:|---:|");
            for clients in [100, 1_000, 10_000] {
                let mut samples = Vec::with_capacity(11);
                for _ in 0..11 {
                    let pool = sqlx::sqlite::SqlitePoolOptions::new()
                        .max_connections(1)
                        .connect("sqlite::memory:")
                        .await?;
                    sqlx::migrate!("./migrations").run(&pool).await?;
                    sqlx::query(
                        "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i+1 < ?) INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at,snapshot,last_modified,status,mode,pxe_mode,keep_writeback,use_game_disk,chap_enabled) SELECT 'pc-'||i,printf('PC%06d',i),printf('02:00:00:%02x:%02x:%02x',(i>>16)&255,(i>>8)&255,i&255),printf('2001:db8::%x',i+1),'diskless/windows11',1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','diskless/windows11@ready','2026-01-01 00:00:00','ready','normal','uefi',1,0,0 FROM n",
                    )
                    .bind(clients)
                    .execute(&pool)
                    .await?;
                    let repository = ClientRepository::new(pool.clone());
                    let start = Instant::now();
                    let result = repository.find_all().await?;
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    assert_eq!(result.len(), clients as usize);
                    assert_eq!(result[0].name, "PC000000");
                    black_box(result);
                    samples.push(elapsed);
                    pool.close().await;
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "| {clients} | {:.3} | {:.3} | {:.3} | 11 |",
                    samples[5], samples[0], samples[10]
                );
            }
            Ok(())
        })
}
