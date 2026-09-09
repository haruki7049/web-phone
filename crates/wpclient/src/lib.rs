pub mod tui;



pub use wpapi::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH, UserAddress};
pub use wpapi::{address, audio, call, config, resample};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_configuration_defaults() {
        let config = Configuration::default();
        assert_eq!(config.server_port, 15000);
        assert!(!config.auto_accept);
        assert!(!config.allow_echoback);
    }

    #[test]
    fn test_client_short_id_validation() {
        let full_addr =
            UserAddress::new("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(full_addr.short_id(), "e3b0c44298fc");
        assert!(full_addr.short_id().len() >= 12);
    }
}
