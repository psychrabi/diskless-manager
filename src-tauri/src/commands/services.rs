use crate::core::service::{ServiceInfo, ServiceManager, ServiceStatus};

#[tauri::command]
pub async fn list_services() -> Result<Vec<ServiceInfo>, String> {
    let manager = ServiceManager::new();
    Ok(manager.list_services())
}

#[tauri::command]
pub async fn get_service_status(name: String) -> Result<ServiceStatus, String> {
    let manager = ServiceManager::new();
    manager.get_status(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_service(name: String) -> Result<String, String> {
    let manager = ServiceManager::new();
    manager.start(&name).map_err(|e| e.to_string())?;
    Ok(format!("Service {} started", name))
}

#[tauri::command]
pub async fn stop_service(name: String) -> Result<String, String> {
    let manager = ServiceManager::new();
    manager.stop(&name).map_err(|e| e.to_string())?;
    Ok(format!("Service {} stopped", name))
}

#[tauri::command]
pub async fn restart_service(name: String) -> Result<String, String> {
    let manager = ServiceManager::new();
    manager.restart(&name).map_err(|e| e.to_string())?;
    Ok(format!("Service {} restarted", name))
}

#[tauri::command]
pub async fn start_all_services() -> Result<String, String> {
    let manager = ServiceManager::new();
    manager.start_all().map_err(|e| e.to_string())?;
    Ok("All services started successfully".to_string())
}

#[tauri::command]
pub async fn stop_all_services() -> Result<String, String> {
    let manager = ServiceManager::new();
    manager.stop_all().map_err(|e| e.to_string())?;
    Ok("All services stopped successfully".to_string())
}
