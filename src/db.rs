//! Database bootstrap using SeaORM.

use crate::config::Config;
use crate::entity::user;
use crate::error::{AppError, AppResult};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait, PaginatorTrait,
    Schema, Set,
};

/// Connect to the database defined by configuration.
pub async fn connect(cfg: &Config) -> AppResult<DatabaseConnection> {
    let db = Database::connect(&cfg.database_url)
        .await
        .map_err(AppError::from)?;
    ensure_schema(&db).await?;
    ensure_default_admin(&db).await?;
    Ok(db)
}

async fn ensure_schema(db: &DatabaseConnection) -> AppResult<()> {
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    let mut stmt = schema.create_table_from_entity(user::Entity);
    stmt.if_not_exists();
    db.execute(builder.build(&stmt)).await?;
    Ok(())
}

fn hash_password_base64(plain: &str) -> Result<String, bcrypt::BcryptError> {
    let hashed = bcrypt::hash(plain, bcrypt::DEFAULT_COST)?;
    Ok(BASE64.encode(hashed.as_bytes()))
}

async fn ensure_default_admin(db: &DatabaseConnection) -> AppResult<()> {
    let count = user::Entity::find().count(db).await?;
    if count == 0 {
        let plain = generate_random_password();
        let hash_b64 =
            hash_password_base64(&plain).map_err(|e| AppError::Config { msg: e.to_string() })?;
        let model = user::ActiveModel {
            username: Set("admin".to_string()),
            password: Set(hash_b64),
            role: Set("admin".to_string()),
            email: Set(Some(String::new())),
            ..Default::default()
        };
        model.insert(db).await?;
        println!("Created default admin user. Please change its password.");
        println!("Username: admin");
        println!("Password: {}", plain);
    }
    Ok(())
}

fn generate_random_password() -> String {
    use rand::distributions::{Alphanumeric, DistString};
    Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
}
