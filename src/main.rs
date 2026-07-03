use std::env;
mod errors;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv_override().ok();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&env::var("DATABASE_URL")?)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    println!("Database set up successfully.");

    Ok(())
}
