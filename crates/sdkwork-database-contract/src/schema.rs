use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::RwLock;

use serde::Deserialize;

use crate::error::ContractError;

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaContract {
    #[serde(rename = "schema_version")]
    pub schema_version: u32,
    pub kind: String,
    #[serde(default = "default_module_id", alias = "owner")]
    pub module_id: String,
    #[serde(default = "default_contract_version", alias = "standard_version")]
    pub contract_version: String,
    #[serde(default)]
    pub table_prefix: String,
    #[serde(default)]
    pub field_sets: BTreeMap<String, Vec<FieldSetColumn>>,
    #[serde(default)]
    pub tables: Vec<TableContract>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldSetColumn {
    pub name: String,
    #[serde(default, rename = "type")]
    pub logical_type: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableContract {
    #[serde(rename = "table_name", alias = "name", default)]
    pub table_name: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default, deserialize_with = "deserialize_columns")]
    pub columns: BTreeMap<String, ColumnContract>,
    #[serde(default)]
    pub constraints: Vec<ConstraintContract>,
    #[serde(default)]
    pub indexes: Vec<IndexContract>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConstraintContract {
    pub name: String,
    #[serde(default, rename = "type")]
    pub constraint_type: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub references_table: Option<String>,
    #[serde(default)]
    pub references_columns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnContract {
    pub name: String,
    #[serde(default, rename = "type")]
    pub logical_type: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexContract {
    pub name: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default, rename = "where", alias = "predicate")]
    pub predicate: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawColumnEntry {
    name: String,
    #[serde(default, rename = "type")]
    logical_type: String,
    #[serde(default)]
    required: bool,
}

fn deserialize_columns<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ColumnContract>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawColumns {
        Map(BTreeMap<String, serde_json::Value>),
        List(Vec<RawColumnEntry>),
    }

    let raw = RawColumns::deserialize(deserializer)?;
    let mut columns = BTreeMap::new();
    match raw {
        RawColumns::Map(map) => {
            for (name, value) in map {
                columns.insert(
                    name.clone(),
                    ColumnContract {
                        name,
                        logical_type: value
                            .get("type")
                            .and_then(|entry| entry.as_str())
                            .unwrap_or("string")
                            .to_string(),
                        required: value
                            .get("required")
                            .and_then(|entry| entry.as_bool())
                            .unwrap_or(false),
                    },
                );
            }
        }
        RawColumns::List(entries) => {
            for entry in entries {
                columns.insert(
                    entry.name.clone(),
                    ColumnContract {
                        name: entry.name.clone(),
                        logical_type: entry.logical_type,
                        required: entry.required,
                    },
                );
            }
        }
    }
    Ok(columns)
}

impl SchemaContract {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ContractError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|error| {
            ContractError::Io(format!(
                "failed to read {}: {error}",
                path.as_ref().display()
            ))
        })?;
        serde_yaml::from_str(&content)
            .map_err(|error| ContractError::Parse(format!("invalid schema yaml: {error}")))
    }

    pub fn expected_table_names(&self) -> Vec<String> {
        self.tables
            .iter()
            .filter_map(|table| table.table_name.clone())
            .collect()
    }

    pub fn expanded_columns_for_table(
        &self,
        table: &TableContract,
    ) -> BTreeMap<String, ColumnContract> {
        let mut merged = BTreeMap::new();
        if let Some(profile) = &table.profile {
            if let Some(fields) = self.field_sets.get(profile) {
                for field in fields {
                    merged.insert(
                        field.name.clone(),
                        ColumnContract {
                            name: field.name.clone(),
                            logical_type: field.logical_type.clone(),
                            required: field.required,
                        },
                    );
                }
            }
        }
        for (name, column) in &table.columns {
            merged.insert(name.clone(), column.clone());
        }
        merged
    }

    pub fn expected_columns_by_table(&self) -> BTreeMap<String, Vec<String>> {
        let mut result = BTreeMap::new();
        for table in &self.tables {
            let Some(table_name) = table.table_name.as_ref() else {
                continue;
            };
            let columns = self.expanded_columns_for_table(table);
            if columns.is_empty() {
                continue;
            }
            result.insert(table_name.clone(), columns.keys().cloned().collect());
        }
        result
    }

    pub fn expected_column_types_by_table(&self) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut result = BTreeMap::new();
        for table in &self.tables {
            let Some(table_name) = table.table_name.as_ref() else {
                continue;
            };
            let columns = self.expanded_columns_for_table(table);
            if columns.is_empty() {
                continue;
            }
            let mut types = BTreeMap::new();
            for (name, column) in columns {
                types.insert(name, column.logical_type);
            }
            result.insert(table_name.clone(), types);
        }
        result
    }

    pub fn expected_constraints_by_table(&self) -> BTreeMap<String, Vec<ConstraintContract>> {
        let mut result = BTreeMap::new();
        for table in &self.tables {
            let Some(table_name) = table.table_name.as_ref() else {
                continue;
            };
            if table.constraints.is_empty() {
                continue;
            }
            result.insert(table_name.clone(), table.constraints.clone());
        }
        result
    }

    pub fn expected_column_required_by_table(&self) -> BTreeMap<String, BTreeMap<String, bool>> {
        let mut result = BTreeMap::new();
        for table in &self.tables {
            let Some(table_name) = table.table_name.as_ref() else {
                continue;
            };
            let columns = self.expanded_columns_for_table(table);
            if columns.is_empty() {
                continue;
            }
            let mut required = BTreeMap::new();
            for (name, column) in columns {
                required.insert(name, column.required);
            }
            result.insert(table_name.clone(), required);
        }
        result
    }

    pub fn expected_indexes_by_table(&self) -> BTreeMap<String, Vec<IndexContract>> {
        let mut result = BTreeMap::new();
        for table in &self.tables {
            let Some(table_name) = table.table_name.as_ref() else {
                continue;
            };
            if table.indexes.is_empty() {
                continue;
            }
            result.insert(table_name.clone(), table.indexes.clone());
        }
        result
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrefixRegistry {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub kind: String,
    #[serde(default)]
    pub prefixes: Vec<PrefixEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrefixEntry {
    pub prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableRegistry {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub kind: String,
    #[serde(default)]
    pub tables: Vec<TableRegistryEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableRegistryEntry {
    pub table_name: String,
}

impl TableRegistry {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ContractError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|error| {
            ContractError::Io(format!(
                "failed to read {}: {error}",
                path.as_ref().display()
            ))
        })?;
        serde_json::from_str(&content)
            .map_err(|error| ContractError::Parse(format!("invalid table registry: {error}")))
    }

    pub fn table_names(&self) -> Vec<String> {
        self.tables
            .iter()
            .map(|entry| entry.table_name.clone())
            .collect()
    }
}

pub fn load_schema_contract(contract_path: &Path) -> Result<SchemaContract, ContractError> {
    SchemaContract::from_file(contract_path)
}

pub fn load_expected_tables(
    contract_path: &Path,
    table_registry_path: &Path,
) -> Result<Vec<String>, ContractError> {
    let mut names = SchemaContract::from_file(contract_path)?.expected_table_names();
    if names.is_empty() && table_registry_path.exists() {
        names = TableRegistry::from_file(table_registry_path)?.table_names();
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn load_expected_columns(
    contract_path: &Path,
) -> Result<BTreeMap<String, Vec<String>>, ContractError> {
    Ok(SchemaContract::from_file(contract_path)?.expected_columns_by_table())
}

pub fn load_expected_column_types(
    contract_path: &Path,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, ContractError> {
    Ok(SchemaContract::from_file(contract_path)?.expected_column_types_by_table())
}

pub fn load_expected_column_required(
    contract_path: &Path,
) -> Result<BTreeMap<String, BTreeMap<String, bool>>, ContractError> {
    Ok(SchemaContract::from_file(contract_path)?.expected_column_required_by_table())
}

pub fn load_expected_indexes(
    contract_path: &Path,
) -> Result<BTreeMap<String, Vec<IndexContract>>, ContractError> {
    Ok(SchemaContract::from_file(contract_path)?.expected_indexes_by_table())
}

pub fn load_expected_constraints(
    contract_path: &Path,
) -> Result<BTreeMap<String, Vec<ConstraintContract>>, ContractError> {
    Ok(SchemaContract::from_file(contract_path)?.expected_constraints_by_table())
}

pub fn normalize_logical_type(logical_type: &str) -> Vec<&'static str> {
    let base = logical_type
        .split('(')
        .next()
        .unwrap_or(logical_type)
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "string" | "text" => vec!["character varying", "varchar", "text"],
        "int64" | "int32" | "integer" => vec!["bigint", "integer", "int"],
        "uuid" => vec!["uuid"],
        "json" | "jsonb" => vec!["json", "jsonb"],
        "instant" | "timestamptz" => vec!["timestamp with time zone", "timestamptz", "text"],
        "decimal" | "numeric" => vec!["numeric", "decimal"],
        "bool" | "boolean" => vec!["boolean", "bool"],
        _ => vec![],
    }
}

pub fn physical_type_matches(logical_type: &str, physical_type: &str) -> bool {
    let physical = physical_type.trim().to_ascii_lowercase();
    if physical.is_empty() {
        return true;
    }
    let aliases = normalize_logical_type(logical_type);
    if aliases.is_empty() {
        return true;
    }
    aliases.iter().any(|alias| physical.contains(alias))
}

fn default_module_id() -> String {
    "default".to_string()
}

fn default_contract_version() -> String {
    "0.1.0".to_string()
}

/// Contract analyzer with caching support.
///
/// This struct caches parsed schema contracts to avoid repeated file I/O
/// and YAML parsing during drift checks. The cache uses file path as key
/// and stores the parsed contract along with its modification time.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_contract::ContractAnalyzer;
/// use std::path::Path;
///
/// let analyzer = ContractAnalyzer::new();
/// let contract = analyzer.load_contract(Path::new("contract/schema.yaml")).unwrap();
///
/// // Subsequent calls will use cached version if file hasn't changed
/// let contract2 = analyzer.load_contract(Path::new("contract/schema.yaml")).unwrap();
/// ```
pub struct ContractAnalyzer {
    cache: RwLock<HashMap<String, CachedContract>>,
}

#[derive(Debug, Clone)]
struct CachedContract {
    contract: SchemaContract,
    tables: Vec<String>,
    columns: BTreeMap<String, Vec<String>>,
    modification_time: std::time::SystemTime,
}

impl ContractAnalyzer {
    /// Create a new contract analyzer with an empty cache.
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Load a schema contract with caching.
    ///
    /// If the contract was previously loaded and the file hasn't been modified,
    /// returns the cached version. Otherwise, parses and caches the new version.
    pub fn load_contract(&self, contract_path: &Path) -> Result<SchemaContract, ContractError> {
        let path_key = contract_path.to_string_lossy().to_string();

        // Check if file exists and get modification time
        let metadata = std::fs::metadata(contract_path)
            .map_err(|e| ContractError::Io(format!("failed to read contract metadata: {}", e)))?;
        let mod_time = metadata
            .modified()
            .map_err(|e| ContractError::Io(format!("failed to get modification time: {}", e)))?;

        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(cached) = cache.get(&path_key) {
                if cached.modification_time == mod_time {
                    return Ok(cached.contract.clone());
                }
            }
        }

        // Load and parse contract
        let contract = SchemaContract::from_file(contract_path)?;
        let tables = contract.expected_table_names();
        let columns = contract.expected_columns_by_table();

        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(
                path_key,
                CachedContract {
                    contract: contract.clone(),
                    tables,
                    columns,
                    modification_time: mod_time,
                },
            );
        }

        Ok(contract)
    }

    /// Get expected tables from cached contract.
    pub fn expected_tables(&self, contract_path: &Path) -> Result<Vec<String>, ContractError> {
        let path_key = contract_path.to_string_lossy().to_string();

        {
            let cache = self.cache.read().unwrap();
            if let Some(cached) = cache.get(&path_key) {
                return Ok(cached.tables.clone());
            }
        }

        // Load if not cached
        self.load_contract(contract_path)?;

        {
            let cache = self.cache.read().unwrap();
            Ok(cache
                .get(&path_key)
                .map(|c| c.tables.clone())
                .unwrap_or_default())
        }
    }

    /// Get expected columns from cached contract.
    pub fn expected_columns(
        &self,
        contract_path: &Path,
    ) -> Result<BTreeMap<String, Vec<String>>, ContractError> {
        let path_key = contract_path.to_string_lossy().to_string();

        {
            let cache = self.cache.read().unwrap();
            if let Some(cached) = cache.get(&path_key) {
                return Ok(cached.columns.clone());
            }
        }

        // Load if not cached
        self.load_contract(contract_path)?;

        {
            let cache = self.cache.read().unwrap();
            Ok(cache
                .get(&path_key)
                .map(|c| c.columns.clone())
                .unwrap_or_default())
        }
    }

    /// Clear the cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Get cache size for monitoring.
    pub fn cache_size(&self) -> usize {
        self.cache.read().unwrap().len()
    }
}

impl Default for ContractAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
