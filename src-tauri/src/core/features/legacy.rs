//! Legacy feature key aliases.

use super::{Feature, Features};

struct Alias {
    legacy_key: &'static str,
    feature: Feature,
}

const ALIASES: &[Alias] = &[
    Alias {
        legacy_key: "connectors",
        feature: Feature::Apps,
    },
    Alias {
        legacy_key: "experimental_use_unified_exec_tool",
        feature: Feature::UnifiedExec,
    },
    Alias {
        legacy_key: "experimental_use_freeform_apply_patch",
        feature: Feature::ApplyPatchFreeform,
    },
    Alias {
        legacy_key: "include_apply_patch_tool",
        feature: Feature::ApplyPatchFreeform,
    },
    Alias {
        legacy_key: "web_search",
        feature: Feature::WebSearchRequest,
    },
    Alias {
        legacy_key: "collab",
        feature: Feature::Collab,
    },
    Alias {
        legacy_key: "memory_tool",
        feature: Feature::MemoryTool,
    },
];

#[allow(dead_code)]
pub(crate) fn legacy_feature_keys() -> impl Iterator<Item = &'static str> {
    ALIASES.iter().map(|alias| alias.legacy_key)
}

pub(crate) fn feature_for_key(key: &str) -> Option<Feature> {
    ALIASES
        .iter()
        .find(|alias| alias.legacy_key == key)
        .map(|alias| alias.feature)
}

#[allow(dead_code)]
#[derive(Debug, Default)]
pub(crate) struct LegacyFeatureToggles {
    pub include_apply_patch_tool: Option<bool>,
    pub experimental_use_freeform_apply_patch: Option<bool>,
    pub experimental_use_unified_exec_tool: Option<bool>,
    pub tools_web_search: Option<bool>,
}

#[allow(dead_code)]
impl LegacyFeatureToggles {
    pub fn apply(self, features: &mut Features) {
        set_if_some(
            features,
            Feature::ApplyPatchFreeform,
            self.include_apply_patch_tool,
        );
        set_if_some(
            features,
            Feature::ApplyPatchFreeform,
            self.experimental_use_freeform_apply_patch,
        );
        set_if_some(
            features,
            Feature::UnifiedExec,
            self.experimental_use_unified_exec_tool,
        );
        set_if_some(features, Feature::WebSearchRequest, self.tools_web_search);
    }
}

#[allow(dead_code)]
fn set_if_some(features: &mut Features, feature: Feature, maybe_value: Option<bool>) {
    if let Some(enabled) = maybe_value {
        if enabled {
            features.enable(feature);
        } else {
            features.disable(feature);
        }
    }
}
