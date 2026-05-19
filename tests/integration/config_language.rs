use jet::config::schema::{EffectiveLanguageSettings, JetConfig};

#[test]
fn language_overrides_apply_to_effective_settings() {
    let mut config = JetConfig {
        tab_width: 4,
        lsp: true,
        highlight: true,
        ..JetConfig::default()
    };
    config.language.insert(
        "rust".to_string(),
        jet::config::schema::LanguageConfig {
            tab_width: Some(2),
            lsp: Some(false),
            highlight: None,
        },
    );

    let rust = config.effective_for_language(Some("rust"));
    assert_eq!(
        rust,
        EffectiveLanguageSettings {
            tab_width: 2,
            lsp: false,
            highlight: true,
        }
    );

    let python = config.effective_for_language(Some("python"));
    assert_eq!(
        python,
        EffectiveLanguageSettings {
            tab_width: 4,
            lsp: true,
            highlight: true,
        }
    );
}
