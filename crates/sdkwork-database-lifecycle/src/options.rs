use std::env;

use sdkwork_database_spi::{DatabaseManifest, LifecycleOptions, LocaleTag, SeedProfile};

pub fn lifecycle_options_from_env(
    service_code: &str,
    manifest: &DatabaseManifest,
) -> LifecycleOptions {
    let prefix = format!("SDKWORK_{}", service_code.to_uppercase());

    LifecycleOptions {
        auto_migrate: read_bool(&format!("{prefix}_DATABASE_AUTO_MIGRATE"))
            .unwrap_or(manifest.lifecycle.auto_migrate),
        seed_on_boot: read_bool(&format!("{prefix}_DATABASE_SEED_ON_BOOT"))
            .unwrap_or(manifest.lifecycle.seed_on_boot),
        seed_locale: LocaleTag(
            env::var(format!("{prefix}_DATABASE_SEED_LOCALE"))
                .unwrap_or_else(|_| manifest.lifecycle.default_seed_locale.clone()),
        ),
        seed_profile: SeedProfile(
            env::var(format!("{prefix}_DATABASE_SEED_PROFILE"))
                .unwrap_or_else(|_| manifest.lifecycle.default_seed_profile.clone()),
        ),
        drift_interval_sec: read_u64(&format!("{prefix}_DATABASE_DRIFT_INTERVAL_SEC"))
            .unwrap_or(manifest.lifecycle.drift_check_interval_sec),
    }
}

fn read_bool(key: &str) -> Option<bool> {
    env::var(key)
        .ok()
        .and_then(|value| match value.to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

fn read_u64(key: &str) -> Option<u64> {
    env::var(key).ok().and_then(|value| value.parse().ok())
}
