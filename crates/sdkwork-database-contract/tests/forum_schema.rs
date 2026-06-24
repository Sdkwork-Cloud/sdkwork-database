use sdkwork_database_contract::{
    load_expected_column_required, load_expected_columns, load_expected_constraints,
    load_expected_indexes, SchemaContract,
};

#[test]
fn parse_forum_style_schema_columns_and_indexes() {
    let yaml = r#"
schema_version: 1
kind: sdkwork.database.schema
owner: forum
standard_version: "1.0.0"
tables:
  - name: forum_space
    columns:
      - { name: code, type: string(64), required: true }
      - { name: slug, type: string(120), required: true }
    indexes:
      - { name: idx_forum_space_tenant_status_updated, columns: [tenant_id, status] }
"#;

    let contract: SchemaContract = serde_yaml::from_str(yaml).expect("forum schema should parse");
    assert_eq!(contract.module_id, "forum");
    assert_eq!(contract.tables.len(), 1);
    assert!(contract.tables[0].columns.contains_key("code"));

    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("schema.yaml");
    std::fs::write(&path, yaml).unwrap();
    let indexes = load_expected_indexes(&path).unwrap();
    assert_eq!(indexes.get("forum_space").unwrap().len(), 1);
}

#[test]
fn profile_field_sets_expand_expected_columns() {
    let yaml = r#"
schema_version: 1
kind: sdkwork.database.schema
owner: forum
standard_version: "1.0.0"
field_sets:
  tenant_entity:
    - { name: tenant_id, type: int64, required: true }
    - { name: status, type: string(32), required: true }
tables:
  - name: forum_space
    profile: tenant_entity
    columns:
      - { name: code, type: string(64), required: true }
"#;

    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("schema.yaml");
    std::fs::write(&path, yaml).unwrap();
    let columns = load_expected_columns(&path).unwrap();
    let forum_space = columns.get("forum_space").expect("forum_space columns");
    assert!(forum_space.iter().any(|name| name == "tenant_id"));
    assert!(forum_space.iter().any(|name| name == "status"));
    assert!(forum_space.iter().any(|name| name == "code"));
}

#[test]
fn parse_constraints_from_forum_style_schema() {
    let yaml = r#"
schema_version: 1
kind: sdkwork.database.schema
owner: forum
standard_version: "1.0.0"
tables:
  - name: forum_space
    constraints:
      - { name: uk_forum_space_uuid, type: unique, columns: [uuid] }
"#;

    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("schema.yaml");
    std::fs::write(&path, yaml).unwrap();
    let constraints = load_expected_constraints(&path).unwrap();
    assert_eq!(constraints.get("forum_space").unwrap().len(), 1);
    assert_eq!(
        constraints.get("forum_space").unwrap()[0].name,
        "uk_forum_space_uuid"
    );
}

#[test]
fn load_expected_column_required_from_profile() {
    let yaml = r#"
schema_version: 1
kind: sdkwork.database.schema
owner: forum
standard_version: "1.0.0"
field_sets:
  tenant_entity:
    - { name: tenant_id, type: int64, required: true }
    - { name: deleted_at, type: instant, required: false }
tables:
  - name: forum_space
    profile: tenant_entity
    columns:
      - { name: code, type: string(64), required: true }
"#;

    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("schema.yaml");
    std::fs::write(&path, yaml).unwrap();
    let required = load_expected_column_required(&path).unwrap();
    let forum_space = required
        .get("forum_space")
        .expect("forum_space required map");
    assert_eq!(forum_space.get("tenant_id"), Some(&true));
    assert_eq!(forum_space.get("deleted_at"), Some(&false));
    assert_eq!(forum_space.get("code"), Some(&true));
}
