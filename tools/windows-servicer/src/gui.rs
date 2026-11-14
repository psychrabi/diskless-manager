use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::windows_client_report::ReporterConfig;
use crate::windows_local_boot::{BootAdapter, LocalBootRequest};

const DEFAULT_CONFIG: &str = r"C:\ProgramData\Diskless Manager\client-report.json";
const DEFAULT_DRIVER_ROOT: &str = r"C:\ProgramData\Diskless Manager\Drivers";

pub fn run() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 800.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    let app = WorkerApp::loaded();
    eframe::run_native(
        "Diskless Manager Windows Client",
        options,
        Box::new(|_creation_context| Ok(Box::new(app))),
    )
    .map_err(|error| anyhow::anyhow!("GUI failed: {error}"))
}

#[derive(Debug)]
enum Operation {
    ListBootNics,
    PrepareBoot(LocalBootRequest),
    Report,
    CollectNic,
    InstallNicDrivers,
    ExportAllDrivers,
    TestConnection,
    Restart,
    RestartElevated,
    Capabilities,
}

/// What a finished background operation reports to the UI.
#[derive(Debug, Clone)]
struct OperationOutcome {
    message: String,
    /// A freshly installed driver needs a reboot before it takes effect.
    reboot_required: bool,
    adapters: Option<Vec<BootAdapter>>,
}

impl OperationOutcome {
    fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            reboot_required: false,
            adapters: None,
        }
    }
}

#[derive(Default)]
struct WorkerApp {
    adapters: Vec<BootAdapter>,
    selected_adapter: String,
    confirm_preparation: bool,
    server_url: String,
    client_id: String,
    config_path: String,
    driver_root: String,
    image_revision: String,
    export_package: bool,
    installation_verification: bool,
    elevated: bool,
    reboot_required: bool,
    status: String,
    busy: bool,
    result_receiver: Option<Receiver<OperationOutcome>>,
}

impl WorkerApp {
    /// Start from the on-disk reporter configuration so the saved server URL,
    /// toggles and test claim survive restarts. A missing or invalid file
    /// leaves the defaults untouched; the first real operation surfaces any
    /// error through the normal reporting path.
    fn loaded() -> Self {
        let mut app = Self::default();
        app.ensure_defaults();
        app.elevated = crate::is_elevated();
        if let Ok(config) = ReporterConfig::load(Path::new(&app.config_path)) {
            app.server_url = config.server_url;
            app.client_id = config.client_id.unwrap_or_default();
            app.export_package = config.export_package;
            app.image_revision = config.image_revision.unwrap_or_default();
            app.installation_verification = config.installation_verification;
            app.status = format!("Loaded configuration from {}", app.config_path);
        }
        app
    }

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
        let client_id = self.client_id.trim().to_string();
        let image_revision = self.image_revision.trim().to_string();
        // Save the complete ReporterConfig contract so toggles the user enabled
        // are never dropped by a hand-built JSON subset.
        let config = ReporterConfig {
            server_url: self.server_url.trim_end_matches('/').to_string(),
            client_id: (!client_id.is_empty()).then_some(client_id),
            export_package: self.export_package,
            image_revision: (!image_revision.is_empty()).then_some(image_revision),
            installation_verification: self.installation_verification,
        };
        let result = (|| -> anyhow::Result<()> {
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
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
        if matches!(
            operation,
            Operation::Report | Operation::CollectNic | Operation::TestConnection
        ) && self.server_url.trim().is_empty()
        {
            self.status = "Enter a server URL before running this operation.".to_string();
            return;
        }
        if matches!(
            operation,
            Operation::Report | Operation::CollectNic | Operation::TestConnection
        ) {
            self.save_config();
            if self.status.starts_with("Failed to save") {
                return;
            }
        }
        if matches!(operation, Operation::RestartElevated) && self.elevated {
            self.status = "This application is already elevated.".to_string();
            return;
        }

        let config_path = PathBuf::from(self.config_path.clone());
        let driver_root = PathBuf::from(self.driver_root.clone());
        let (sender, receiver) = mpsc::channel::<OperationOutcome>();
        self.result_receiver = Some(receiver);
        self.busy = true;
        self.status = "Running operation...".to_string();
        thread::spawn(move || {
            let outcome = run_operation(operation, &config_path, &driver_root);
            let _ = sender.send(outcome);
        });
    }
}

fn run_operation(operation: Operation, config_path: &Path, driver_root: &Path) -> OperationOutcome {
    let result: anyhow::Result<OperationOutcome> = match operation {
        Operation::ListBootNics => crate::windows_local_boot::list_adapters().map(|adapters| OperationOutcome {
            message: format!("Found {} wired PCI adapter(s). Select the NIC used for LAN boot.", adapters.len()),
            reboot_required: false,
            adapters: Some(adapters),
        }),
        Operation::PrepareBoot(ref request) => crate::windows_local_boot::prepare(request).map(|result| {
            let mut message = if result.applied {
                format!("Windows boot preparation completed for {}.\nRegistry backup: {}\nRestart from the local disk, then shut down fully and capture/import the image on the server. Configure the client for PXE/iPXE iSCSI boot.\n", result.adapter.name, result.backup_path.as_deref().unwrap_or_default())
            } else { format!("Preview for {} (no changes applied):\n", result.adapter.name) };
            for change in &result.plan.changes {
                message.push_str(&format!("{}\\{} = {:?}\n", change.key, change.name, change.value));
            }
            for warning in &result.plan.warnings { message.push_str(&format!("\n{warning}")); }
            OperationOutcome { message, reboot_required: result.reboot_required, adapters: None }
        }),
        Operation::Report => crate::windows_client_report::run_report_client(config_path)
            .map(|()| OperationOutcome::info("Client report uploaded successfully.")),
        Operation::CollectNic => crate::windows_client_report::run_collect_nic(config_path)
            .map(|()| OperationOutcome::info("NIC packages collected and uploaded successfully.")),
        Operation::InstallNicDrivers => {
            crate::windows_client_report::install_nic_drivers(driver_root).map(|summary| {
                let mut message = format!(
                    "Processed {} INF package(s): {} installed, {} had no matching device.",
                    summary.inf_count,
                    summary.installed_count(),
                    summary.no_matching_device_count()
                );
                if summary.reboot_required() {
                    message.push_str(" Restart the client to start using the new drivers.");
                }
                let per_package = summary
                    .packages
                    .iter()
                    .map(|package| format!("{} — {}", package.inf, package.outcome))
                    .collect::<Vec<_>>()
                    .join("\n");
                OperationOutcome {
                    message: format!("{message}\n{per_package}"),
                    reboot_required: summary.reboot_required(),
                    adapters: None,
                }
            })
        }
        Operation::ExportAllDrivers => {
            crate::windows_client_report::collect_all_drivers(driver_root).map(|count| {
                OperationOutcome::info(format!(
                    "Exported {count} third-party driver package(s) to {}.",
                    driver_root.display()
                ))
            })
        }
        Operation::TestConnection => {
            crate::windows_client_report::test_server_connection(config_path)
                .map(|()| OperationOutcome::info("Server connection test succeeded."))
        }
        Operation::Restart => crate::windows_client_report::restart_client()
            .map(|()| OperationOutcome::info("Restarting the client in 5 seconds...")),
        Operation::RestartElevated => crate::windows_client_report::restart_elevated().map(|()| {
            OperationOutcome::info("Relaunching this application with administrator rights...")
        }),
        Operation::Capabilities => Ok(OperationOutcome::info(
            serde_json::to_string_pretty(&crate::capabilities())
                .unwrap_or_else(|_| "{}".to_string()),
        )),
    };
    match result {
        Ok(outcome) => outcome,
        Err(error) => {
            let mut message = format!("Operation failed: {error:#}");
            // Only point at the client log when this operation writes one.
            match operation {
                Operation::Report
                | Operation::CollectNic
                | Operation::TestConnection
                | Operation::InstallNicDrivers
                | Operation::ExportAllDrivers => {
                    let base = match operation {
                        Operation::InstallNicDrivers | Operation::ExportAllDrivers => {
                            driver_root.parent().unwrap_or(driver_root)
                        }
                        _ => config_path.parent().unwrap_or_else(|| Path::new(".")),
                    };
                    message.push_str(&format!(
                        "\nDetails: {}",
                        crate::windows_client_report::log_path(base).display()
                    ));
                }
                Operation::Restart
                | Operation::RestartElevated
                | Operation::Capabilities
                | Operation::ListBootNics
                | Operation::PrepareBoot(_) => {}
            }
            OperationOutcome::info(message)
        }
    }
}

impl eframe::App for WorkerApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(receiver) = &self.result_receiver {
            if let Ok(outcome) = receiver.try_recv() {
                if let Some(adapters) = outcome.adapters {
                    self.adapters = adapters;
                    self.selected_adapter.clear();
                    self.confirm_preparation = false;
                }
                self.status = outcome.message;
                self.busy = false;
                // Never withdraw a pending-reboot hint on its own: a later
                // operation that reports none still leaves the drivers waiting.
                self.reboot_required |= outcome.reboot_required;
                self.result_receiver = None;
            }
        }
        self.ensure_defaults();
        context.request_repaint_after(std::time::Duration::from_millis(100));
        if self.busy && context.input(|input| input.viewport().close_requested()) {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        egui::CentralPanel::default().show(context, |ui| {
          egui::ScrollArea::vertical().id_salt("client-page").show(ui, |ui| {
            ui.heading("Diskless Manager Windows Client");
            ui.label("Install signed NIC drivers, then prepare this Windows installation for diskless iSCSI boot.");
            ui.label(if self.elevated {
                "Running elevated."
            } else {
                "Not running elevated; driver installation needs elevation."
            });
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
                    ui.label("Export driver packages");
                    ui.checkbox(&mut self.export_package, "");
                    ui.end_row();
                    ui.label("Installation verification");
                    ui.checkbox(&mut self.installation_verification, "Shut down when verified");
                    ui.end_row();
                    ui.label("Image revision (test claim)");
                    ui.text_edit_singleline(&mut self.image_revision);
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                if ui.button("Save configuration").clicked() {
                    self.save_config();
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Test server connection"))
                    .clicked()
                {
                    self.start(Operation::TestConnection);
                }
                if ui
                    .add_enabled(!self.busy && !self.elevated, egui::Button::new("Restart elevated"))
                    .clicked()
                {
                    self.start(Operation::RestartElevated);
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
                    .add_enabled(
                        !self.busy && self.elevated,
                        egui::Button::new("Install local NIC drivers"),
                    )
                    .clicked()
                {
                    self.start(Operation::InstallNicDrivers);
                }
                if ui
                    .add_enabled(
                        !self.busy && self.elevated,
                        egui::Button::new("Export all driver packages"),
                    )
                    .clicked()
                {
                    self.start(Operation::ExportAllDrivers);
                }
                if ui
                    .add_enabled(!self.busy && self.reboot_required, egui::Button::new("Restart now"))
                    .clicked()
                {
                    self.start(Operation::Restart);
                }
            });

            ui.separator();
            ui.heading("Prepare for diskless boot");
            ui.label("Use a reference Windows 10/11 installation with the intended Ethernet NIC installed. This action changes this PC's boot settings.");
            ui.add_enabled_ui(!self.busy, |ui| {
                if ui.button("Refresh boot adapters").clicked() { self.start(Operation::ListBootNics); }
                let selected_name = self.adapters.iter().find(|nic| nic.id == self.selected_adapter)
                    .map(|nic| format!("{} — {}", nic.name, nic.mac))
                    .unwrap_or_else(|| "Select the boot NIC".into());
                egui::ComboBox::from_label("Boot NIC").selected_text(selected_name).show_ui(ui, |ui| {
                    for adapter in &self.adapters {
                        if ui.selectable_value(&mut self.selected_adapter, adapter.id.clone(),
                            format!("{} | {} | {} | {}", adapter.name, adapter.description, adapter.mac, adapter.status)).changed() {
                            self.confirm_preparation = false;
                        }
                    }
                });
                ui.checkbox(&mut self.confirm_preparation, "I want to change this Windows installation's boot settings.");
                ui.horizontal_wrapped(|ui| {
                    let selected = !self.selected_adapter.is_empty() && self.elevated && !self.reboot_required;
                    if ui.add_enabled(selected, egui::Button::new("Preview preparation")).clicked() {
                        self.start(Operation::PrepareBoot(LocalBootRequest { adapter_id: self.selected_adapter.clone(), apply: false }));
                    }
                    if ui.add_enabled(selected && self.confirm_preparation, egui::Button::new("Prepare for diskless boot")).clicked() {
                        self.start(Operation::PrepareBoot(LocalBootRequest { adapter_id: self.selected_adapter.clone(), apply: true }));
                        self.confirm_preparation = false;
                    }
                });
            });
            ui.small("Install NIC drivers first. Preparation saves original registry values and requires a restart. Capture/import the image on the server after a full shutdown.");
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
        });
    }
}
