use clap::Args;
use sqlx::migrate::Migrator;
use std::path::PathBuf;

#[derive(Args)]
pub struct MigrateArgs {
    #[arg(long, env = "DATABASE_URL", default_value = "postgres://coppice:coppice@localhost:5432/coppice")]
    pub database_url: String,
}

pub async fn run(args: MigrateArgs) -> anyhow::Result<()> {
    let migrations = migrations_path()?;
    let migrator = Migrator::new(migrations).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&args.database_url)
        .await?;
    migrator.run(&pool).await?;
    println!("migrations applied");
    Ok(())
}

fn migrations_path() -> anyhow::Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../server/migrations");
    if path.exists() {
        return Ok(path);
    }
    anyhow::bail!("migrations path not found at {}", path.display())
}
