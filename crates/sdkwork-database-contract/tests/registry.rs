use sdkwork_database_contract::load_expected_tables;
use tempfile::TempDir;

#[test]
fn load_expected_tables_from_registry() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    std::fs::write(
        root.join("schema.yaml"),
        "schema_version: 1\nkind: sdkwork.database.schema\nmodule_id: demo\ncontract_version: 0.1.0\ntables: []\n",
    )
    .unwrap();
    std::fs::write(
        root.join("table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"demo_users"}]}"#,
    )
    .unwrap();

    let tables =
        load_expected_tables(&root.join("schema.yaml"), &root.join("table-registry.json")).unwrap();
    assert_eq!(tables, vec!["demo_users".to_string()]);
}
