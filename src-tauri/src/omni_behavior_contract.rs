//! Clean-room behavioral fixtures derived from public Steam layout rules and the
//! OmniPacker v1.4.0 release notes. No OmniPacker source is linked or copied.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
struct DepotFixture {
    depot_id: u32,
    manifest_id: u64,
    owner_app_id: u32,
    display_name: String,
    shared: bool,
    source_manifest_present: bool,
}

#[derive(Debug, Clone)]
struct AppFixture {
    app_id: u32,
    install_dir: String,
    build_id: u64,
    depots: Vec<DepotFixture>,
    install_script: Option<String>,
}

fn preflight(fixture: &AppFixture) -> Result<Vec<DepotFixture>, String> {
    if fixture.app_id == 0 || fixture.build_id == 0 {
        return Err("AppID and BuildID must be positive".to_string());
    }
    if fixture.install_dir.is_empty()
        || fixture.install_dir == "."
        || fixture.install_dir == ".."
        || fixture.install_dir.contains('/')
        || fixture.install_dir.contains('\\')
    {
        return Err("installDir must be one literal steamapps/common child".to_string());
    }
    let mut by_depot = BTreeMap::<u32, DepotFixture>::new();
    for depot in &fixture.depots {
        if depot.depot_id == 0 || depot.manifest_id == 0 || depot.owner_app_id == 0 {
            return Err("Depot identity is incomplete".to_string());
        }
        if !depot.source_manifest_present {
            return Err(format!(
                "Manifest {}_{}.manifest is missing before commit",
                depot.depot_id, depot.manifest_id
            ));
        }
        if depot.shared && depot.owner_app_id == fixture.app_id {
            return Err(format!(
                "Shared depot {} lost its source owner identity",
                depot.depot_id
            ));
        }
        if depot.display_name.trim().is_empty() {
            return Err(format!(
                "Depot {} has no source display identity",
                depot.depot_id
            ));
        }
        match by_depot.get(&depot.depot_id) {
            Some(previous) if previous != depot => {
                return Err(format!(
                    "Conflicting mapping for depot {} was rejected before commit",
                    depot.depot_id
                ))
            }
            Some(_) => {}
            None => {
                by_depot.insert(depot.depot_id, depot.clone());
            }
        }
    }
    Ok(by_depot.into_values().collect())
}

fn render_appmanifest(fixture: &AppFixture) -> Result<String, String> {
    let depots = preflight(fixture)?;
    let mut output = format!(
        "\"AppState\"\n{{\n\t\"appid\"\t\t\"{}\"\n\t\"installdir\"\t\t\"{}\"\n\t\"buildid\"\t\t\"{}\"\n\t\"InstalledDepots\"\n\t{{\n",
        fixture.app_id, fixture.install_dir, fixture.build_id
    );
    for depot in depots {
        output.push_str(&format!(
            "\t\t\"{}\"\n\t\t{{\n\t\t\t\"manifest\"\t\t\"{}\"\n\t\t}}\n",
            depot.depot_id, depot.manifest_id
        ));
    }
    output.push_str("\t}\n");
    if let Some(script) = fixture.install_script.as_deref() {
        output.push_str(&format!("\t\"InstallScripts\"\t\t\"{}\"\n", script));
    }
    output.push_str("}\n");
    Ok(output)
}

fn manifest_destinations(depots: &[DepotFixture]) -> BTreeSet<String> {
    depots
        .iter()
        .map(|depot| {
            format!(
                "depotcache/{}_{}.manifest",
                depot.depot_id, depot.manifest_id
            )
        })
        .collect()
}

fn base_fixture() -> AppFixture {
    AppFixture {
        app_id: 100,
        install_dir: "The Crust Deluxe".to_string(),
        build_id: 9001,
        depots: vec![
            DepotFixture {
                depot_id: 101,
                manifest_id: 111,
                owner_app_id: 100,
                display_name: "The Crust Deluxe Content".to_string(),
                shared: false,
                source_manifest_present: true,
            },
            DepotFixture {
                depot_id: 228_990,
                manifest_id: 222,
                owner_app_id: 228_980,
                display_name: "Steamworks Common Redistributables".to_string(),
                shared: true,
                source_manifest_present: true,
            },
            DepotFixture {
                depot_id: 201,
                manifest_id: 333,
                owner_app_id: 200,
                display_name: "The Crust Soundtrack".to_string(),
                shared: true,
                source_manifest_present: true,
            },
        ],
        install_script: None,
    }
}

#[test]
fn preserves_install_dir_spaces_and_renders_exact_acf_identities() {
    let fixture = base_fixture();
    let acf = render_appmanifest(&fixture).unwrap();
    assert!(acf.contains("\"appid\"\t\t\"100\""));
    assert!(acf.contains("\"installdir\"\t\t\"The Crust Deluxe\""));
    assert!(!acf.contains("The.Crust.Deluxe"));
    assert!(acf.contains("\"buildid\"\t\t\"9001\""));
    assert!(acf.contains("\"101\""));
    assert!(acf.contains("\"manifest\"\t\t\"111\""));
}

#[test]
fn deduplicates_identical_shared_depots_and_keeps_owner_names() {
    let mut fixture = base_fixture();
    fixture.depots.push(fixture.depots[1].clone());
    let depots = preflight(&fixture).unwrap();
    assert_eq!(
        depots
            .iter()
            .filter(|depot| depot.depot_id == 228_990)
            .count(),
        1
    );
    let redistributable = depots
        .iter()
        .find(|depot| depot.depot_id == 228_990)
        .unwrap();
    assert_eq!(redistributable.owner_app_id, 228_980);
    assert_eq!(
        redistributable.display_name,
        "Steamworks Common Redistributables"
    );
    let dlc = depots.iter().find(|depot| depot.depot_id == 201).unwrap();
    assert_eq!(dlc.display_name, "The Crust Soundtrack");
    assert_ne!(dlc.display_name, fixture.install_dir);
}

#[test]
fn uses_exact_depotcache_names_and_only_emits_source_install_script() {
    let mut fixture = base_fixture();
    let destinations = manifest_destinations(&fixture.depots);
    assert!(destinations.contains("depotcache/101_111.manifest"));
    assert!(destinations.contains("depotcache/228990_222.manifest"));
    assert!(!render_appmanifest(&fixture)
        .unwrap()
        .contains("InstallScripts"));
    fixture.install_script = Some("installscript.vdf".to_string());
    assert!(render_appmanifest(&fixture)
        .unwrap()
        .contains("\"InstallScripts\"\t\t\"installscript.vdf\""));
}

#[test]
fn preflight_rejects_missing_wrong_or_conflicting_manifest_mapping() {
    let mut missing = base_fixture();
    missing.depots[0].source_manifest_present = false;
    assert!(preflight(&missing)
        .unwrap_err()
        .contains("missing before commit"));

    let mut conflict = base_fixture();
    let mut conflicting = conflict.depots[0].clone();
    conflicting.manifest_id = 999;
    conflict.depots.push(conflicting);
    assert!(preflight(&conflict)
        .unwrap_err()
        .contains("Conflicting mapping"));

    let mut wrong_owner = base_fixture();
    wrong_owner.depots[1].owner_app_id = wrong_owner.app_id;
    assert!(preflight(&wrong_owner)
        .unwrap_err()
        .contains("owner identity"));
}

#[test]
fn launcher_chunk_staging_path_implementation_was_not_replaced_by_packer_logic() {
    let paths_source = include_str!("job/paths.rs");
    assert!(paths_source.contains("downloading_dir_for_install"));
    assert!(paths_source.contains("staged_chunk_dir"));
    assert!(!paths_source.contains("OmniPacker"));
    assert!(!paths_source.contains("app_local_data_dir"));
}
