use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const DEFAULT_CONFIG: &str = r"C:\ProgramData\Diskless Manager\client-report.json";
const DEFAULT_DRIVER_ROOT: &str = r"C:\ProgramData\Diskless Manager\Drivers";

pub fn run() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Diskless Manager Windows Worker",
        options,
        Box::new(|_creation_context| Ok(Box::new(WorkerApp::default()))),
    )
    .map_err(|error| anyhow::anyhow!("GUI failed: {error}"))
}

#[derive(Debug)]
enum Operation {
    Report,
    CollectNic,
    InstallNicDrivers,
    Capabilities,
}

#[derive(Default)]
struct WorkerApp {
    server_url: String,
    client_id: String,
    config_path: String,
    driver_root: String,
    status: String,
    busy: bool,
    result_receiver: Option<Receiver<String>>,
}

impl WorkerApp {
    fn ensure_defaults(&mut self) {
        if self.config_path.is_empty() {
            self.config_path = DEFAULT_CONFIG.to_string();
        }
        if self.driver_root.is_empty() {
            self.driver_root = DEFAULT_DRIVER_ROOT.to_string();
        }
    }

    fn save_config(&mut self) {
        self.ensure_defaults();
        let config_path = PathBuf::from(&self.config_path);
        let result = (|| -> anyhow::Result<()> {
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let config = serde_json::json!({
                "server_url": self.server_url.trim_end_matches('/'),
                "client_id": if self.client_id.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(self.client_id.trim().to_string()) },
                "export_package": false,
            });
            std::fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
            Ok(())
        })();
        self.status = match result {
            Ok(()) => format!("Saved client configuration to {}", config_path.display()),
            Err(error) => format!("Failed to save configuration: {error:#}"),
        };
    }

    fn start(&mut self, operation: Operation) {
        if self.busy {
            return;
        }
        self.ensure_defaults();
        if matches!(operation, Operation::Report | Operation::CollectNic)
            && self.server_url.trim().is_empty()
        {
            self.status = "Enter a server URL before running this operation.".to_string();
            return;
        }
        if matches!(operation, Operation::Report | Operation::CollectNic) {
            self.save_config();
            if self.status.starts_with("Failed to save") {
                return;
            }
        }

        let config_path = PathBuf::from(self.config_path.clone());
        let driver_root = PathBuf::from(self.driver_root.clone());
        let (sender, receiver) = mpsc::channel();
        self.result_receiver = Some(receiver);
        self.busy = true;
        self.status = "Running operation...".to_string();
        thread::spawn(move || {
            let result = run_operation(operation, &config_path, &driver_root);
            let _ = sender.send(result);
        });
    }
}

fn run_operation(operation: Operation, config_path: &Path, driver_root: &Path) -> String {
    let result = match operation {
        Operation::Report => crate::windows_client_report::run_report_client(config_path)
            .map(|()| "Client report uploaded successfully.".to_string()),
        Operation::CollectNic => crate::windows_client_report::run_collect_nic(config_path)
            .map(|()| "NIC packages collected and uploaded successfully.".to_string()),
        Operation::InstallNicDrivers => {
            crate::windows_client_report::install_nic_drivers(driver_root).map(|summary| {
                format!(
                    "Installed {} INF package(s); reboot required: {}",
                    summary.inf_count, summary.reboot_required
                )
            })
        }
        Operation::Capabilities => {
            let capabilities = serde_json::json!({
                "protocol_version": 3,
                "platform": std::env::consts::OS,
                "available": crate::windows_servicing_available(),
                "elevated": crate::is_elevated(),
                "iscsi_volume_servicing": cfg!(windows),
                "nexboot_boot_time": cfg!(windows),
                "nic_collection": cfg!(windows),
                "local_nic_installation": cfg!(windows),
            });
            Ok(serde_json::to_string_pretty(&capabilities).unwrap_or_else(|_| "{}".to_string()))
        }
    };
    match result {
        Ok(message) => message,
        Err(error) => format!("Operation failed: {error:#}"),
    }
}

impl eframe::App for WorkerApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(receiver) = &self.result_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.status = result;
                self.busy = false;
                self.result_receiver = None;
            }
        }
        self.ensure_defaults();
        context.request_repaint_after(std::time::Duration::from_millis(100));

        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("Diskless Manager Windows Worker");
            ui.label("Use this application on a Windows client for NIC collection, reporting, and local driver installation.");
            ui.separator();

            egui::Grid::new("worker-settings")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Server URL");
                    ui.text_edit_singleline(&mut self.server_url);
                    ui.end_row();
                    ui.label("Client ID");
                    ui.text_edit_singleline(&mut self.client_id);
                    ui.end_row();
                    ui.label("Report config");
                    ui.text_edit_singleline(&mut self.config_path);
                    ui.end_row();
                    ui.label("Driver directory");
                    ui.text_edit_singleline(&mut self.driver_root);
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                if ui.button("Save configuration").clicked() {
                    self.save_config();
                }
                if ui.button("Capabilities").clicked() {
                    self.start(Operation::Capabilities);
                }
            });

            ui.separator();
            ui.heading("Client operations");
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Report client"))
                    .clicked()
                {
                    self.start(Operation::Report);
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Collect and upload NICs"))
                    .clicked()
                {
                    self.start(Operation::CollectNic);
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Install local NIC drivers"))
                    .clicked()
                {
                    self.start(Operation::InstallNicDrivers);
                }
            });

            ui.separator();
            ui.label("Status");
            egui::ScrollArea::vertical()
                .max_height(210.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.monospace(&self.status);
                });
            if self.busy {
                ui.add(egui::ProgressBar::new(f32::NAN).text("Working..."));
            }
            ui.separator();
            ui.small("The command-line protocol remains available for server-side SSH servicing. Run this executable with no arguments to open this window.");
        });
    }
}
