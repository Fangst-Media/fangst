//! Layer and license rules for the Fangst workspace. `main.rs` feeds this from `cargo metadata`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Level(u8),
    /// `fangst-core`: plumbing any layer above 0 may use.
    Core,
    /// Dev tools and test helpers: may use anything, only reachable through dev-dependencies.
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
}

#[derive(Debug, Clone)]
pub struct WorkspaceCrate {
    pub name: String,
    pub license: Option<String>,
    /// Dependencies on other workspace crates.
    pub deps: Vec<(String, DepKind)>,
    /// Licenses of every external crate reachable through normal dependencies.
    pub external_licenses: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    Unmapped {
        krate: String,
    },
    UpwardDependency {
        krate: String,
        krate_layer: Layer,
        dep: String,
        dep_layer: Layer,
    },
    ToolInNormalDependency {
        krate: String,
        dep: String,
    },
    CoreDependsOnWorkspace {
        dep: String,
    },
    Layer0DependsOnCore {
        krate: String,
    },
    Layer0OwnLicense {
        krate: String,
        license: Option<String>,
    },
    Layer0CopyleftDependency {
        krate: String,
        dep: String,
        license: Option<String>,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unmapped { krate } => {
                write!(f, "{krate} is not in layers.toml; add it to a layer")
            }
            Self::UpwardDependency {
                krate,
                krate_layer,
                dep,
                dep_layer,
            } => write!(
                f,
                "{krate} ({}) depends on {dep} ({}); crates may only depend on lower layers",
                describe(*krate_layer),
                describe(*dep_layer)
            ),
            Self::ToolInNormalDependency { krate, dep } => {
                write!(
                    f,
                    "{krate} depends on tool {dep}; tools may only be dev-dependencies"
                )
            }
            Self::CoreDependsOnWorkspace { dep } => {
                write!(
                    f,
                    "fangst-core depends on {dep}; core must not depend on other workspace crates"
                )
            }
            Self::Layer0DependsOnCore { krate } => write!(
                f,
                "{krate} (layer 0) depends on fangst-core; layer 0 is MIT/Apache and core is AGPL"
            ),
            Self::Layer0OwnLicense { krate, license } => write!(
                f,
                "{krate} (layer 0) is licensed {}; layer 0 crates must be `MIT OR Apache-2.0`",
                license.as_deref().unwrap_or("<none>")
            ),
            Self::Layer0CopyleftDependency {
                krate,
                dep,
                license,
            } => write!(
                f,
                "{krate} (layer 0) pulls in {dep} licensed {}; layer 0 takes only permissive dependencies",
                license.as_deref().unwrap_or("<unknown>")
            ),
        }
    }
}

fn describe(layer: Layer) -> String {
    match layer {
        Layer::Level(n) => format!("layer {n}"),
        Layer::Core => "core".to_owned(),
        Layer::Tool => "tool".to_owned(),
    }
}

pub const LAYER0_LICENSE: &str = "MIT OR Apache-2.0";

/// Crate name patterns to layers, as written in `layers.toml`. A pattern ending in `*` matches by prefix.
#[derive(Debug, Default)]
pub struct LayerMap {
    entries: Vec<(String, Layer)>,
}

impl LayerMap {
    pub fn parse(text: &str) -> Result<Self, String> {
        #[derive(serde::Deserialize)]
        struct File {
            core: Vec<String>,
            tools: Vec<String>,
            layers: BTreeMap<String, Vec<String>>,
        }
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        entries.extend(file.core.into_iter().map(|n| (n, Layer::Core)));
        entries.extend(file.tools.into_iter().map(|n| (n, Layer::Tool)));
        for (level, names) in file.layers {
            let level: u8 = level
                .parse()
                .map_err(|_| format!("layer key `{level}` is not a number"))?;
            entries.extend(names.into_iter().map(|n| (n, Layer::Level(level))));
        }
        Ok(Self { entries })
    }

    #[must_use]
    pub fn layer_of(&self, name: &str) -> Option<Layer> {
        // Exact names win over patterns so one crate can be carved out of a family.
        self.entries
            .iter()
            .find(|(pattern, _)| pattern == name)
            .or_else(|| {
                self.entries.iter().find(|(pattern, _)| {
                    pattern
                        .strip_suffix('*')
                        .is_some_and(|prefix| name.starts_with(prefix))
                })
            })
            .map(|(_, layer)| *layer)
    }
}

/// True when at least one alternative of an SPDX `OR` expression is free of GPL-family terms.
#[must_use]
pub fn is_permissive(license: Option<&str>) -> bool {
    let Some(license) = license else { return false };
    license
        .split(" OR ")
        .map(|alt| alt.trim_matches(|c: char| c == '(' || c == ')' || c.is_whitespace()))
        .any(|alt| !alt.contains("GPL"))
}

#[must_use]
pub fn check(crates: &[WorkspaceCrate], map: &LayerMap) -> Vec<Violation> {
    let mut violations = Vec::new();
    let layers: BTreeMap<&str, Layer> = crates
        .iter()
        .filter_map(|c| map.layer_of(&c.name).map(|l| (c.name.as_str(), l)))
        .collect();

    for krate in crates {
        let Some(&layer) = layers.get(krate.name.as_str()) else {
            violations.push(Violation::Unmapped {
                krate: krate.name.clone(),
            });
            continue;
        };

        let mut seen = BTreeSet::new();
        for (dep, kind) in &krate.deps {
            if !seen.insert((dep.as_str(), *kind == DepKind::Dev)) {
                continue;
            }
            let Some(&dep_layer) = layers.get(dep.as_str()) else {
                continue;
            };
            if let Some(v) = edge_violation(&krate.name, layer, dep, dep_layer, *kind) {
                violations.push(v);
            }
        }

        if layer == Layer::Level(0) {
            if krate.license.as_deref() != Some(LAYER0_LICENSE) {
                violations.push(Violation::Layer0OwnLicense {
                    krate: krate.name.clone(),
                    license: krate.license.clone(),
                });
            }
            for (dep, license) in &krate.external_licenses {
                if !is_permissive(license.as_deref()) {
                    violations.push(Violation::Layer0CopyleftDependency {
                        krate: krate.name.clone(),
                        dep: dep.clone(),
                        license: license.clone(),
                    });
                }
            }
        }
    }
    violations
}

fn edge_violation(
    krate: &str,
    layer: Layer,
    dep: &str,
    dep_layer: Layer,
    kind: DepKind,
) -> Option<Violation> {
    if layer == Layer::Tool {
        return None;
    }
    if dep_layer == Layer::Tool {
        return (kind == DepKind::Normal).then(|| Violation::ToolInNormalDependency {
            krate: krate.to_owned(),
            dep: dep.to_owned(),
        });
    }
    match (layer, dep_layer) {
        (Layer::Core, _) => Some(Violation::CoreDependsOnWorkspace {
            dep: dep.to_owned(),
        }),
        (Layer::Level(0), Layer::Core) => Some(Violation::Layer0DependsOnCore {
            krate: krate.to_owned(),
        }),
        (Layer::Level(a), Layer::Level(b)) if b > a || (b == a && a != 0) => {
            Some(Violation::UpwardDependency {
                krate: krate.to_owned(),
                krate_layer: layer,
                dep: dep.to_owned(),
                dep_layer,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"
        core = ["fangst-core"]
        tools = ["layercheck", "fangst-bench"]
        [layers]
        0 = ["fangst-nntp", "fangst-yenc", "fangst-nzb"]
        1 = ["fangst-usenet", "fangst-notify-*"]
        2 = ["fangst-downloads", "fangst-manager"]
        3 = ["fangst-dl-api"]
        4 = ["fangst-dl"]
    "#;

    fn krate(name: &str, deps: &[(&str, DepKind)]) -> WorkspaceCrate {
        let layer0 = ["fangst-nntp", "fangst-yenc", "fangst-nzb"].contains(&name);
        WorkspaceCrate {
            name: name.to_owned(),
            license: Some(
                if layer0 {
                    LAYER0_LICENSE
                } else {
                    "AGPL-3.0-or-later"
                }
                .to_owned(),
            ),
            deps: deps.iter().map(|(d, k)| ((*d).to_owned(), *k)).collect(),
            external_licenses: Vec::new(),
        }
    }

    fn run(crates: &[WorkspaceCrate]) -> Vec<Violation> {
        check(crates, &LayerMap::parse(MAP).unwrap())
    }

    use DepKind::{Dev, Normal};

    #[test]
    fn clean_workspace_passes() {
        let crates = [
            krate("fangst-core", &[]),
            krate("fangst-yenc", &[]),
            krate("fangst-nzb", &[("fangst-yenc", Normal)]),
            krate(
                "fangst-usenet",
                &[
                    ("fangst-nzb", Normal),
                    ("fangst-core", Normal),
                    ("fangst-bench", Dev),
                ],
            ),
            krate("fangst-downloads", &[("fangst-usenet", Normal)]),
            krate("fangst-dl", &[("fangst-downloads", Normal)]),
            krate("fangst-bench", &[("fangst-nntp", Normal)]),
            krate("fangst-nntp", &[]),
        ];
        assert_eq!(run(&crates), Vec::new());
    }

    #[test]
    fn upward_dependency_names_both_crates() {
        let crates = [
            krate("fangst-usenet", &[]),
            krate("fangst-nzb", &[("fangst-usenet", Normal)]),
        ];
        let violations = run(&crates);
        assert_eq!(violations.len(), 1);
        let message = violations[0].to_string();
        assert!(
            message.contains("fangst-nzb") && message.contains("fangst-usenet"),
            "{message}"
        );
    }

    #[test]
    fn same_layer_above_zero_is_refused() {
        let crates = [
            krate("fangst-downloads", &[("fangst-manager", Normal)]),
            krate("fangst-manager", &[]),
        ];
        assert!(matches!(
            run(&crates)[..],
            [Violation::UpwardDependency { .. }]
        ));
    }

    #[test]
    fn same_layer_zero_is_allowed() {
        let crates = [
            krate("fangst-nzb", &[("fangst-yenc", Normal)]),
            krate("fangst-yenc", &[]),
        ];
        assert_eq!(run(&crates), Vec::new());
    }

    #[test]
    fn upward_dev_dependency_is_refused_too() {
        let crates = [
            krate("fangst-usenet", &[("fangst-dl", Dev)]),
            krate("fangst-dl", &[]),
        ];
        assert!(matches!(
            run(&crates)[..],
            [Violation::UpwardDependency { .. }]
        ));
    }

    #[test]
    fn tool_as_normal_dependency_is_refused() {
        let crates = [
            krate("fangst-usenet", &[("fangst-bench", Normal)]),
            krate("fangst-bench", &[]),
        ];
        assert!(matches!(
            run(&crates)[..],
            [Violation::ToolInNormalDependency { .. }]
        ));
    }

    #[test]
    fn layer0_may_not_use_core() {
        let crates = [
            krate("fangst-yenc", &[("fangst-core", Normal)]),
            krate("fangst-core", &[]),
        ];
        assert!(matches!(
            run(&crates)[..],
            [Violation::Layer0DependsOnCore { .. }]
        ));
    }

    #[test]
    fn core_may_not_use_workspace_crates() {
        let crates = [
            krate("fangst-core", &[("fangst-yenc", Normal)]),
            krate("fangst-yenc", &[]),
        ];
        assert!(matches!(
            run(&crates)[..],
            [Violation::CoreDependsOnWorkspace { .. }]
        ));
    }

    #[test]
    fn unmapped_crate_is_reported() {
        assert!(matches!(
            run(&[krate("fangst-mystery", &[])])[..],
            [Violation::Unmapped { .. }]
        ));
    }

    #[test]
    fn prefix_pattern_maps_a_family() {
        let map = LayerMap::parse(MAP).unwrap();
        assert_eq!(map.layer_of("fangst-notify-ntfy"), Some(Layer::Level(1)));
    }

    #[test]
    fn layer0_with_gpl_dependency_fails() {
        let mut yenc = krate("fangst-yenc", &[]);
        yenc.external_licenses = vec![("rarpar".to_owned(), Some("GPL-3.0".to_owned()))];
        let violations = run(&[yenc]);
        assert!(
            matches!(&violations[..], [Violation::Layer0CopyleftDependency { dep, .. }] if dep == "rarpar")
        );
    }

    #[test]
    fn layer0_must_be_mit_or_apache() {
        let mut yenc = krate("fangst-yenc", &[]);
        yenc.license = Some("AGPL-3.0-or-later".to_owned());
        assert!(matches!(
            run(&[yenc])[..],
            [Violation::Layer0OwnLicense { .. }]
        ));
    }

    #[test]
    fn gpl_dependency_above_layer0_is_fine() {
        let mut usenet = krate("fangst-usenet", &[]);
        usenet.external_licenses = vec![("rarpar".to_owned(), Some("GPL-3.0".to_owned()))];
        assert_eq!(run(&[usenet]), Vec::new());
    }

    #[test]
    fn dual_license_with_permissive_option_is_permissive() {
        assert!(is_permissive(Some("MIT OR GPL-2.0")));
        assert!(is_permissive(Some("(MIT OR Apache-2.0) AND Unicode-3.0")));
        assert!(!is_permissive(Some("LGPL-2.1-or-later")));
        assert!(!is_permissive(Some("AGPL-3.0")));
        assert!(!is_permissive(None));
    }
}
