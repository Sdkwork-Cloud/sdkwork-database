use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sdkwork_database_config::DatabaseEngine;

use crate::drift_policy::DriftPolicyFile;
use crate::error::SpiError;
use crate::manifest::DatabaseManifest;
use crate::seed_manifest::SeedManifest;
use crate::traits::{
    DatabaseAssetProvider, DatabaseContractProvider, DatabaseModule,
    DatabaseModuleDescriptorProvider, DriftPolicyProvider, MigrationProvider, SeedProvider,
};
use crate::types::{
    DatabaseModuleDescriptor, DriftPolicy, LocaleTag, MigrationSpec, SeedPlan, SeedProfile,
};

#[derive(Debug, Clone)]
pub struct DefaultDatabaseModule {
    module_root: PathBuf,
    manifest: DatabaseManifest,
}

impl DefaultDatabaseModule {
    pub fn from_module_root(module_root: impl AsRef<Path>) -> Result<Self, SpiError> {
        let module_root = module_root.as_ref().to_path_buf();
        let manifest = DatabaseManifest::from_file(module_root.join("database.manifest.json"))?;
        Ok(Self {
            module_root,
            manifest,
        })
    }

    pub fn from_app_root(app_root: impl AsRef<Path>) -> Result<Self, SpiError> {
        Self::from_module_root(app_root.as_ref().join("database"))
    }

    pub fn from_manifest(
        app_root: impl AsRef<Path>,
        manifest_path: impl AsRef<Path>,
    ) -> Result<Self, SpiError> {
        let manifest_path = manifest_path.as_ref();
        let manifest = DatabaseManifest::from_file(manifest_path)?;
        let module_root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| app_root.as_ref().to_path_buf());
        Ok(Self {
            module_root,
            manifest,
        })
    }

    pub fn manifest(&self) -> &DatabaseManifest {
        &self.manifest
    }

    pub fn module_root(&self) -> &Path {
        &self.module_root
    }
}

impl DatabaseModuleDescriptorProvider for DefaultDatabaseModule {
    fn descriptor(&self) -> DatabaseModuleDescriptor {
        DatabaseModuleDescriptor {
            module_id: self.manifest.module_id.clone(),
            service_code: self.manifest.service_code.clone(),
            table_prefix: self.manifest.table_prefix.clone(),
            supported_engines: parse_manifest_engines(&self.manifest),
        }
    }
}

fn parse_manifest_engines(manifest: &DatabaseManifest) -> Vec<DatabaseEngine> {
    if manifest.engines.is_empty() {
        return vec![DatabaseEngine::Postgres, DatabaseEngine::Sqlite];
    }

    manifest
        .engines
        .iter()
        .filter_map(|engine| match engine.to_lowercase().as_str() {
            "postgres" | "postgresql" => Some(DatabaseEngine::Postgres),
            "sqlite" => Some(DatabaseEngine::Sqlite),
            _ => None,
        })
        .collect()
}

impl DatabaseAssetProvider for DefaultDatabaseModule {
    fn module_root(&self) -> &Path {
        &self.module_root
    }

    fn manifest_path(&self) -> PathBuf {
        self.module_root.join("database.manifest.json")
    }

    fn contract_path(&self) -> PathBuf {
        self.module_root.join(&self.manifest.paths.contract)
    }

    fn migrations_dir(&self, engine: DatabaseEngine) -> PathBuf {
        let engine_dir = match engine {
            DatabaseEngine::Postgres => "postgres",
            DatabaseEngine::Sqlite => "sqlite",
        };
        self.module_root
            .join(&self.manifest.paths.migrations)
            .join(engine_dir)
    }

    fn seeds_dir(&self) -> PathBuf {
        self.module_root.join(&self.manifest.paths.seeds)
    }

    fn drift_policy_path(&self) -> PathBuf {
        self.module_root.join(&self.manifest.paths.drift_policy)
    }
}

#[async_trait]
impl DatabaseContractProvider for DefaultDatabaseModule {
    async fn contract_version(&self) -> Result<String, SpiError> {
        Ok(self.manifest.contract_version.clone())
    }
}

#[async_trait]
impl MigrationProvider for DefaultDatabaseModule {
    async fn list_migrations(
        &self,
        engine: DatabaseEngine,
    ) -> Result<Vec<MigrationSpec>, SpiError> {
        let dir = self.migrations_dir(engine);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut migrations = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|error| {
            SpiError::Migration(format!("failed to read migrations dir: {error}"))
        })? {
            let entry = entry.map_err(|error| {
                SpiError::Migration(format!("failed to read migration entry: {error}"))
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("sql") {
                continue;
            }
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !file_name.ends_with(".up.sql") {
                continue;
            }
            let stem = file_name.trim_end_matches(".up.sql");
            let mut parts = stem.splitn(2, '_');
            let version = parts.next().unwrap_or(stem).to_string();
            let name = parts.next().unwrap_or(stem).to_string();
            let down_path = dir.join(format!("{stem}.down.sql"));
            migrations.push(MigrationSpec {
                version,
                name,
                engine,
                up_path: path,
                down_path: down_path.exists().then_some(down_path),
                checksum: None,
            });
        }

        migrations.sort_by(|left, right| left.version.cmp(&right.version));
        Ok(migrations)
    }
}

#[async_trait]
impl SeedProvider for DefaultDatabaseModule {
    async fn resolve_seed_plan(
        &self,
        locale: &LocaleTag,
        profile: &SeedProfile,
    ) -> Result<SeedPlan, SpiError> {
        if !self
            .manifest
            .lifecycle
            .active_seed_locales
            .iter()
            .any(|value| value == &locale.0)
        {
            return Err(SpiError::Seed(format!(
                "locale {} is not active for module {}",
                locale.0, self.manifest.module_id
            )));
        }

        let seed_manifest = SeedManifest::from_file(self.seeds_dir().join("seed.manifest.json"))?;
        seed_manifest.resolve_plan(&self.seeds_dir(), locale, profile)
    }
}

#[async_trait]
impl DriftPolicyProvider for DefaultDatabaseModule {
    async fn load_policy(&self) -> Result<DriftPolicy, SpiError> {
        let path = self.drift_policy_path();
        if path.exists() {
            return Ok(DriftPolicyFile::from_file(path)?.into_policy());
        }
        Ok(DriftPolicy::default())
    }
}

#[async_trait]
impl DatabaseModule for DefaultDatabaseModule {}
