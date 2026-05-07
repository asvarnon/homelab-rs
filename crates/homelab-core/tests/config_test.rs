use homelab_core::config::Config;
use std::path::PathBuf;

#[test]
fn test_config_loading() {
    // Use CARGO_MANIFEST_DIR to find the fixture reliably, regardless of where the test is run from.
    let mut config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    config_path.push("tests/fixtures/config.toml");

    let config = Config::load(&config_path).expect("Failed to load fixture config");

    // Test Proxmox
    let proxmox = config.endpoints.get("proxmox").expect("Proxmox endpoint not found");
    assert_eq!(proxmox.url, "https://10.10.10.1:8006");
    
    if let homelab_core::config::AuthConfig::ApiToken { id_env, secret_env } = &proxmox.auth {
        assert_eq!(id_env, "PROXMOX_TOKEN_ID");
        assert_eq!(secret_env, "PROXMOX_TOKEN_SECRET");
    } else {
        panic!("Expected ApiToken auth for proxmox, got {:?}", proxmox.auth);
    }

    // Test OPNsense
    let opnsense = config.endpoints.get("opnsense").expect("OPNsense endpoint not found");
    assert_eq!(opnsense.url, "https://10.10.10.1");
    if let homelab_core::config::AuthConfig::Basic { user_env, pass_env } = &opnsense.auth {
        assert_eq!(user_env, "OPNSENSE_API_KEY");
        assert_eq!(pass_env, "OPNSENSE_API_SECRET");
    } else {
        panic!("Expected Basic auth for opnsense, got {:?}", opnsense.auth);
    }

    // Test Llama (None)
    let llama = config.endpoints.get("llama").expect("Llama endpoint not found");
    assert!(matches!(llama.auth, homelab_core::config::AuthConfig::None));
}
