use toml::Value as TomlValue;

/// Merge TOML `overlay` into `base`, giving `overlay` precedence.
/// Tables are merged recursively; scalars/arrays in overlay replace base.
pub fn merge_toml_values(base: &mut TomlValue, overlay: &TomlValue) {
    match (base.is_table(), overlay.is_table()) {
        (true, true) => {
            let overlay_table = overlay.as_table().unwrap();
            let base_table = base.as_table_mut().unwrap();
            for (key, value) in overlay_table {
                if let Some(existing) = base_table.get_mut(key) {
                    merge_toml_values(existing, value);
                } else {
                    base_table.insert(key.clone(), value.clone());
                }
            }
        }
        _ => {
            *base = overlay.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_overlay_wins() {
        let mut base: TomlValue = toml::from_str("model = \"gpt-3.5\"").unwrap();
        let overlay: TomlValue = toml::from_str("model = \"gpt-4\"").unwrap();
        merge_toml_values(&mut base, &overlay);
        assert_eq!(base["model"].as_str(), Some("gpt-4"));
    }

    #[test]
    fn tables_merge_recursively() {
        let mut base: TomlValue =
            toml::from_str("[tui]\ntheme = \"dark\"\nalt_screen = \"auto\"").unwrap();
        let overlay: TomlValue = toml::from_str("[tui]\ntheme = \"light\"").unwrap();
        merge_toml_values(&mut base, &overlay);
        assert_eq!(base["tui"]["theme"].as_str(), Some("light"));
        assert_eq!(base["tui"]["alt_screen"].as_str(), Some("auto"));
    }

    #[test]
    fn new_keys_added() {
        let mut base: TomlValue = toml::from_str("model = \"gpt-4\"").unwrap();
        let overlay: TomlValue = toml::from_str("instructions = \"be helpful\"").unwrap();
        merge_toml_values(&mut base, &overlay);
        assert_eq!(base["model"].as_str(), Some("gpt-4"));
        assert_eq!(base["instructions"].as_str(), Some("be helpful"));
    }
}
