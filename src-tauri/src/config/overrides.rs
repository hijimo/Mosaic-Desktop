use toml::Value as TomlValue;

/// Build a TOML layer from CLI `--set key=value` overrides.
/// Each entry is a dotted path (e.g. `"model"` or `"tui.theme"`) and a TOML value.
pub fn build_cli_overrides_layer(cli_overrides: &[(String, TomlValue)]) -> TomlValue {
    let mut root = TomlValue::Table(Default::default());
    for (path, value) in cli_overrides {
        apply_toml_override(&mut root, path, value.clone());
    }
    root
}

fn apply_toml_override(root: &mut TomlValue, path: &str, value: TomlValue) {
    let mut current = root;
    let mut segments = path.split('.').peekable();

    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            // Last segment — set the value.
            match current {
                TomlValue::Table(table) => {
                    table.insert(segment.to_string(), value);
                }
                _ => {
                    let mut table = toml::map::Map::new();
                    table.insert(segment.to_string(), value);
                    *current = TomlValue::Table(table);
                }
            }
            return;
        }

        // Intermediate segment — ensure a table exists.
        match current {
            TomlValue::Table(table) => {
                current = table
                    .entry(segment.to_string())
                    .or_insert_with(|| TomlValue::Table(Default::default()));
            }
            _ => {
                *current = TomlValue::Table(Default::default());
                if let TomlValue::Table(tbl) = current {
                    current = tbl
                        .entry(segment.to_string())
                        .or_insert_with(|| TomlValue::Table(Default::default()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_override() {
        let layer = build_cli_overrides_layer(&[
            ("model".into(), TomlValue::String("gpt-4o".into())),
        ]);
        assert_eq!(layer["model"].as_str(), Some("gpt-4o"));
    }

    #[test]
    fn nested_override() {
        let layer = build_cli_overrides_layer(&[
            ("tui.theme".into(), TomlValue::String("dark".into())),
        ]);
        assert_eq!(layer["tui"]["theme"].as_str(), Some("dark"));
    }

    #[test]
    fn multiple_overrides() {
        let layer = build_cli_overrides_layer(&[
            ("model".into(), TomlValue::String("gpt-4".into())),
            ("tui.theme".into(), TomlValue::String("light".into())),
        ]);
        assert_eq!(layer["model"].as_str(), Some("gpt-4"));
        assert_eq!(layer["tui"]["theme"].as_str(), Some("light"));
    }
}
