use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use layercheck::{DepKind, LayerMap, WorkspaceCrate, check};
use serde::Deserialize;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    license: Option<String>,
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    name: String,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<DepKindInfo>,
}

#[derive(Deserialize)]
struct DepKindInfo {
    kind: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(0) => {
            println!("layercheck: all crates follow the layer and license rules");
            ExitCode::SUCCESS
        }
        Ok(count) => {
            eprintln!("layercheck: {count} violation(s)");
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("layercheck: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<usize, String> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .map_err(|e| format!("could not run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("could not parse cargo metadata: {e}"))?;

    let map_path = metadata.workspace_root.join("layers.toml");
    let map_text =
        std::fs::read_to_string(&map_path).map_err(|e| format!("{}: {e}", map_path.display()))?;
    let map = LayerMap::parse(&map_text).map_err(|e| format!("{}: {e}", map_path.display()))?;

    let crates = workspace_crates(&metadata);
    let violations = check(&crates, &map);
    for violation in &violations {
        eprintln!("  {violation}");
    }
    Ok(violations.len())
}

fn workspace_crates(metadata: &Metadata) -> Vec<WorkspaceCrate> {
    let members: BTreeSet<&str> = metadata
        .workspace_members
        .iter()
        .map(String::as_str)
        .collect();
    let packages: BTreeMap<&str, &Package> = metadata
        .packages
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let member_names: BTreeSet<&str> = members
        .iter()
        .filter_map(|id| packages.get(id))
        .map(|p| p.name.as_str())
        .collect();
    let normal_deps: BTreeMap<&str, Vec<&str>> = metadata
        .resolve
        .nodes
        .iter()
        .map(|node| {
            let deps = node
                .deps
                .iter()
                // Build dependencies run at compile time and are not shipped, so only normal ones count.
                .filter(|d| d.dep_kinds.iter().any(|k| k.kind.is_none()))
                .map(|d| d.pkg.as_str())
                .collect();
            (node.id.as_str(), deps)
        })
        .collect();

    members
        .iter()
        .filter_map(|id| packages.get(id))
        .map(|package| WorkspaceCrate {
            name: package.name.clone(),
            license: package.license.clone(),
            deps: package
                .dependencies
                .iter()
                .filter(|d| member_names.contains(d.name.as_str()))
                .filter(|d| d.kind.as_deref() != Some("build"))
                .map(|d| {
                    let kind = if d.kind.as_deref() == Some("dev") {
                        DepKind::Dev
                    } else {
                        DepKind::Normal
                    };
                    (d.name.clone(), kind)
                })
                .collect(),
            external_licenses: reachable(&package.id, &normal_deps)
                .into_iter()
                .filter(|id| !members.contains(id))
                .filter_map(|id| packages.get(id))
                .map(|p| (p.name.clone(), p.license.clone()))
                .collect(),
        })
        .collect()
}

fn reachable<'a>(start: &'a str, graph: &BTreeMap<&'a str, Vec<&'a str>>) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        for &dep in graph.get(id).into_iter().flatten() {
            if seen.insert(dep) {
                stack.push(dep);
            }
        }
    }
    seen
}
