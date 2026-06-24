use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use sdkwork_database_drift::DriftEngine;
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::{
    traits::SeedProvider, validate_module_layout, DefaultDatabaseModule, LocaleTag, SeedProfile,
};
use sdkwork_database_sqlx::{create_pool_from_env, DatabasePool};

#[derive(Parser)]
#[command(name = "sdkwork-db", about = "SDKWork database lifecycle CLI")]
struct Cli {
    #[arg(long, default_value = ".")]
    app_root: PathBuf,
    #[arg(short, long, env = "SDKWORK_DATABASE_SERVICE_CODE")]
    service: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate database module layout under app_root/database
    Validate,
    /// Create ops history tables and apply schema migrations on empty databases
    Init,
    /// List pending migrations, seed plan, and drift summary without applying
    Plan {
        #[arg(long)]
        locale: Option<String>,
        #[arg(long)]
        profile: Option<String>,
    },
    /// Apply pending migrations
    Migrate,
    /// Apply seed scripts for locale/profile
    Seed {
        #[arg(long, default_value = "zh-CN")]
        locale: String,
        #[arg(long, default_value = "standard")]
        profile: String,
    },
    /// Migrate then seed
    Bootstrap {
        #[arg(long, default_value = "zh-CN")]
        locale: String,
        #[arg(long, default_value = "standard")]
        profile: String,
    },
    /// Print lifecycle status summary
    Status,
    /// Print drift report JSON
    Drift,
    /// Exit non-zero when error-level drift exists
    DriftCheck,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let module = Arc::new(DefaultDatabaseModule::from_app_root(&cli.app_root)?);

    match cli.command {
        Commands::Validate => {
            if let Err(failures) = validate_module_layout(module.module_root()) {
                for failure in failures {
                    eprintln!("validate failed: {failure}");
                }
                std::process::exit(1);
            }
            println!("validate passed");
        }
        command => {
            let service_code = cli
                .service
                .unwrap_or_else(|| module.manifest().service_code.clone());
            let pool = resolve_pool(&service_code).await?;
            let orchestrator = LifecycleOrchestrator::new(pool.clone(), module.clone());

            match command {
                Commands::Init => {
                    orchestrator.init().await?;
                    let count = orchestrator.migrate().await?;
                    println!("init complete: {} migration(s) applied", count);
                }
                Commands::Plan { locale, profile } => {
                    let manifest = module.manifest();
                    let locale = LocaleTag(
                        locale.unwrap_or_else(|| manifest.lifecycle.default_seed_locale.clone()),
                    );
                    let profile = SeedProfile(
                        profile.unwrap_or_else(|| manifest.lifecycle.default_seed_profile.clone()),
                    );
                    let pending = orchestrator.plan_migrations().await?;
                    let seed_plan = module.resolve_seed_plan(&locale, &profile).await?;
                    let drift = DriftEngine::new(pool.clone(), module.clone())
                        .analyze()
                        .await?;
                    let output = serde_json::json!({
                        "pending_migrations": pending
                            .iter()
                            .map(|m| format!("{}_{}", m.version, m.name))
                            .collect::<Vec<_>>(),
                        "seed_plan": {
                            "locale": locale.0,
                            "profile": profile.0,
                            "common_scripts": seed_plan.common_scripts,
                            "locale_scripts": seed_plan.locale_scripts,
                        },
                        "drift_summary": {
                            "status": drift.status,
                            "error": drift.summary.error,
                            "warn": drift.summary.warn,
                            "info": drift.summary.info,
                        },
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
                Commands::Migrate => {
                    let count = orchestrator.migrate().await?;
                    println!("applied {} migration(s)", count);
                }
                Commands::Seed { locale, profile } => {
                    let count = orchestrator
                        .seed(&LocaleTag(locale), &SeedProfile(profile))
                        .await?;
                    println!("applied {} seed script(s)", count);
                }
                Commands::Bootstrap { locale, profile } => {
                    let (migrations, seeds) = orchestrator
                        .bootstrap(&LocaleTag(locale), &SeedProfile(profile))
                        .await?;
                    println!(
                        "bootstrap complete: {} migration(s), {} seed script(s)",
                        migrations, seeds
                    );
                }
                Commands::Status => {
                    let drift = DriftEngine::new(pool, module).analyze().await?;
                    println!(
                        "module={} engine={} status={} pending_migrations={}",
                        drift.module_id,
                        drift.engine,
                        drift.status,
                        drift.pending_migrations.len()
                    );
                }
                Commands::Drift => {
                    let report = DriftEngine::new(pool, module).analyze().await?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                Commands::DriftCheck => {
                    let report = DriftEngine::new(pool, module).analyze().await?;
                    if report.summary.error > 0 {
                        eprintln!(
                            "drift check failed: {} error-level diff(s)",
                            report.summary.error
                        );
                        std::process::exit(1);
                    }
                    println!("drift check passed");
                }
                Commands::Validate => unreachable!(),
            }
        }
    }

    Ok(())
}

async fn resolve_pool(service_code: &str) -> anyhow::Result<DatabasePool> {
    match create_pool_from_env(service_code).await? {
        Some(pool) => Ok(pool),
        None => anyhow::bail!(
            "database URL not configured; set SDKWORK_{}_DATABASE_URL",
            service_code
        ),
    }
}
