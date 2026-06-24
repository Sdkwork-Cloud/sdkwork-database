use std::sync::Arc;

use crate::error::SpiError;
use crate::traits::DatabaseModule;

#[derive(Default)]
pub struct DatabaseModuleRegistry {
    modules: Vec<Arc<dyn DatabaseModule>>,
}

impl DatabaseModuleRegistry {
    pub fn builder() -> DatabaseModuleRegistryBuilder {
        DatabaseModuleRegistryBuilder::default()
    }

    pub fn modules(&self) -> &[Arc<dyn DatabaseModule>] {
        &self.modules
    }
}

#[derive(Default)]
pub struct DatabaseModuleRegistryBuilder {
    modules: Vec<Arc<dyn DatabaseModule>>,
}

impl DatabaseModuleRegistryBuilder {
    pub fn register<M>(mut self, module: M) -> Result<Self, SpiError>
    where
        M: DatabaseModule + 'static,
    {
        self.modules.push(Arc::new(module));
        Ok(self)
    }

    pub fn build(self) -> DatabaseModuleRegistry {
        DatabaseModuleRegistry {
            modules: self.modules,
        }
    }
}
