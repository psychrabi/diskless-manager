use axum::{http::StatusCode, Json};
use log::info;

use crate::{
    auth::authenticate_user,
    types::{LoginRequest, LoginResponse},
};

pub async fn login(Json(request): Json<LoginRequest>) -> Result<Json<LoginResponse>, StatusCode> {
    let username = request.username.clone();
    let password = request.password.clone();

    info!("login attempt: user={}", username);

    let auth_result = authenticate_user(&username, &password);

    match auth_result {
        Ok(response) => {
            info!("login success: user={}", username);
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}
