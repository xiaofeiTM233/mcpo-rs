use actix_web::http::header;
use actix_web::web;
use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    Error,
};

#[allow(dead_code)]
pub fn check_api_key(req: &ServiceRequest, expected_key: &str) -> bool {
    if expected_key.is_empty() {
        return true;
    }
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|key| key == expected_key)
}

#[allow(dead_code)]
pub async fn verify_api_key(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let app_key = req
        .app_data::<web::Data<String>>()
        .map(|k| k.get_ref().clone());

    if let Some(ref key) = app_key {
        if !key.is_empty() {
            let valid = req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .is_some_and(|auth| auth == key.as_str());

            if !valid {
                return Err(actix_web::error::ErrorUnauthorized(
                    "Invalid or missing API key. Use 'Authorization: Bearer <api-key>' header.",
                ));
            }
        }
    }

    next.call(req).await
}
