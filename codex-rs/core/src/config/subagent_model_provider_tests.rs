use codex_config::CONFIG_TOML_FILE;
use codex_config::ProfileV2Name;

use super::Config;

async fn config_with_base_allowlist() -> Config {
    let mut config = super::super::test_config().await;
    config.config_layer_stack = config
        .config_layer_stack
        .with_user_config(
            &config.codex_home.join(CONFIG_TOML_FILE),
            toml::from_str("subagent_model_provider_allowlist = [\"external\"]")
                .expect("valid base user config"),
        )
        .expect("base user config should be accepted");
    config
}

#[tokio::test]
async fn user_allowlist_follows_profile_precedence_without_falling_back() {
    let mut config = config_with_base_allowlist().await;
    let profile: ProfileV2Name = "worker".parse().expect("valid profile name");
    let profile_path = config.codex_home.join("profiles/worker.toml");

    config.config_layer_stack = config
        .config_layer_stack
        .with_user_config_profile(
            &profile_path,
            Some(&profile),
            toml::from_str("model = \"gpt-5.6-sol\"").expect("valid profile user config"),
        )
        .expect("profile user config should be accepted");
    assert!(config.user_allows_subagent_model_provider("external"));

    config.config_layer_stack = config
        .config_layer_stack
        .with_user_config_profile(
            &profile_path,
            Some(&profile),
            toml::from_str("subagent_model_provider_allowlist = []")
                .expect("valid profile user config"),
        )
        .expect("profile user config should be accepted");
    assert!(!config.user_allows_subagent_model_provider("external"));

    config.config_layer_stack = config
        .config_layer_stack
        .with_user_config_profile(
            &profile_path,
            Some(&profile),
            toml::from_str("subagent_model_provider_allowlist = \"invalid\"")
                .expect("syntactically valid profile user config"),
        )
        .expect("profile layer should be accepted before typed config validation");
    assert!(!config.user_allows_subagent_model_provider("external"));
}
