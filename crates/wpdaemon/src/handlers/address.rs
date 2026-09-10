//! Address resolution and listing HTTP handlers.

use axum::extract::{Json, Path};
use wpapi::UserAddress;
use crate::connection::{find_client_by_address, get_client_address, get_registered_addresses};
use crate::error::SignalingError;
use crate::registry::AddressSearchResult;

/// Handler to retrieve all registered wpclient user addresses (WPIP-02 Section 2.3).
pub async fn list_registered_addresses() -> Json<Vec<UserAddress>> {
    Json(get_registered_addresses())
}

/// Handler to resolve a client UserAddress by Short ID prefix (WPIP-02 Section 5).
pub async fn resolve_registered_address(
    Path(id): Path<String>,
) -> Result<Json<UserAddress>, SignalingError> {
    let dummy_addr = UserAddress::new(id);
    match find_client_by_address(&dummy_addr) {
        AddressSearchResult::Found(cid) => {
            let addr = get_client_address(cid).ok_or_else(|| {
                SignalingError::NotFound("Client address not found".into())
            })?;
            Ok(Json(addr))
        }
        AddressSearchResult::Ambiguous => Err(SignalingError::AddressAmbiguous),
        AddressSearchResult::NotFound => {
            Err(SignalingError::NotFound("Address prefix not found".into()))
        }
    }
}
