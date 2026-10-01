pub use app_lib::{application, core, metrics, ssh_executor, state, types};
// Inside the library these handlers use the real private auth module.
mod auth {
    pub fn jwt_secret() -> &'static [u8] {
        b"test-signing-secret-at-least-32-bytes"
    }
}
#[path = "../src/api/handlers/backup.rs"]
mod backup;
