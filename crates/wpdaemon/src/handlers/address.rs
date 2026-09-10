use crate::config::CONFIGURATION;
use crate::connection::{find_client_by_address, get_client_address, get_registered_addresses};
use crate::error::SignalingError;
use crate::registry::AddressSearchResult;
use axum::extract::{Json, Path};
use axum::http::HeaderMap;
use wpapi::UserAddress;

fn check_address_request_authorization(headers: &HeaderMap) -> Result<(), SignalingError> {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .or_else(|| headers.get("X-WebPhone-Sign"))
        .and_then(|v| v.to_str().ok());

    if let Some(auth_val) = auth_header {
        wpapi::verify_any_authorization_header(auth_val, "")
            .map_err(SignalingError::Unauthorized)?;
        Ok(())
    } else if daemon_config.allow_anonymous {
        Ok(())
    } else {
        Err(SignalingError::Unauthorized(
            wpapi::AuthError::MissingHeader,
        ))
    }
}

/// Handler to retrieve all registered wpclient user addresses (WPIP-02 Section 2.3).
pub async fn list_registered_addresses(
    headers: HeaderMap,
) -> Result<Json<Vec<UserAddress>>, SignalingError> {
    check_address_request_authorization(&headers)?;
    Ok(Json(get_registered_addresses()))
}

/// Handler to resolve a client UserAddress by Short ID prefix (WPIP-02 Section 5).
pub async fn resolve_registered_address(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<UserAddress>, SignalingError> {
    check_address_request_authorization(&headers)?;
    let dummy_addr = UserAddress::new(id);
    match find_client_by_address(&dummy_addr) {
        AddressSearchResult::Found(cid) => {
            let addr = get_client_address(cid)
                .ok_or_else(|| SignalingError::NotFound("Client address not found".into()))?;
            Ok(Json(addr))
        }
        AddressSearchResult::Ambiguous => Err(SignalingError::AddressAmbiguous),
        AddressSearchResult::NotFound => {
            Err(SignalingError::NotFound("Address prefix not found".into()))
        }
    }
}
