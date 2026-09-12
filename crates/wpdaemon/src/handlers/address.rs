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
    } else if daemon_config.server.allow_anonymous {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[tokio::test]
    async fn test_list_and_resolve_address_handlers_unauthorized() {
        let headers = HeaderMap::new();
        let res = list_registered_addresses(headers.clone()).await;
        assert!(res.is_err());

        let res_resolve = resolve_registered_address(headers, Path("nonexistent".into())).await;
        assert!(res_resolve.is_err());
    }

    #[tokio::test]
    async fn test_address_handlers_with_valid_auth_header() {
        let keypair1 = wpapi::UserKeypair::generate();

        // Header for list request
        let (_, auth_str1) = wpapi::build_authorization_header(&keypair1, "");
        let mut headers1 = HeaderMap::new();
        headers1.insert(
            axum::http::header::AUTHORIZATION,
            auth_str1.parse().unwrap(),
        );

        let res = list_registered_addresses(headers1).await;
        assert!(res.is_ok());

        // Header for resolve request (using keypair2 to guarantee distinct signature)
        let keypair2 = wpapi::UserKeypair::generate();
        let (_, auth_str2) = wpapi::build_authorization_header(&keypair2, "");
        let mut headers2 = HeaderMap::new();
        headers2.insert(
            axum::http::header::AUTHORIZATION,
            auth_str2.parse().unwrap(),
        );

        // Resolve not found address returns 404
        let res_resolve = resolve_registered_address(headers2, Path("nonexistent_id".into())).await;
        assert!(matches!(res_resolve, Err(SignalingError::NotFound(_))));
    }
}
